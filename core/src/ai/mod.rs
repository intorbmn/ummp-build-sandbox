//! Application: AI agent system (Phase 7).
//!
//! Provider-agnostic: agents talk to an [`AiProvider`] trait; HTTP providers
//! live in the separate `ummp-ai-providers` crate so the core has no network
//! dependency. AI output is only ever a *proposal*: edits are validated,
//! reviewed, approved by a human (or an explicit policy) and applied by the
//! migration engine after a checkpoint (ADR 0005, ADR 0010).

pub mod agents;
pub mod orchestrator;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// The agents of Phase 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Analysis,
    Migration,
    Debug,
    Review,
}

impl AgentKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::Migration => "migration",
            Self::Debug => "debug",
            Self::Review => "review",
        }
    }
}

/// One structured-output request. `schema` is a JSON Schema the response
/// must follow (every object has `additionalProperties: false`).
#[derive(Debug, Clone, Serialize)]
pub struct AiRequest {
    pub model: String,
    /// `low` | `medium` | `high` | `xhigh` | `max` (ignored by providers
    /// that have no effort control).
    pub effort: String,
    pub max_tokens: u32,
    pub system: String,
    pub user: String,
    pub schema: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl Usage {
    pub const fn total(self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, Clone)]
pub struct AiResponse {
    /// The JSON text produced by the model.
    pub text: String,
    pub usage: Usage,
    /// Model that actually answered (may differ after a server-side fallback).
    pub model: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AiError {
    /// Rate limit, overload, 5xx, timeout: worth retrying.
    #[error("transient provider error: {0}")]
    Transient(String),
    /// Bad request, auth, not found: retrying will not help.
    #[error("provider error: {0}")]
    Fatal(String),
    #[error("the model declined the request: {0}")]
    Refused(String),
    #[error("output truncated at max_tokens")]
    Truncated,
    #[error("invalid model output: {0}")]
    InvalidOutput(String),
    #[error("token budget exhausted: {spent} of {budget} tokens used")]
    BudgetExceeded { spent: u64, budget: u64 },
}

/// A model provider. Implementations must not log or persist credentials.
pub trait AiProvider: Send + Sync {
    fn name(&self) -> &str;
    fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiError>;
}

// ---------------------------------------------------------------------------
// Model routing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentModel {
    pub model: String,
    pub effort: String,
    pub max_tokens: u32,
}

/// Per-agent model settings, retry/fallback policy and budget.
/// Loadable from YAML/JSON (`config::load_any`); every field has a default.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub agents: BTreeMap<AgentKind, AgentModel>,
    /// Model tried once more when retries on the primary are exhausted.
    pub fallback_model: Option<String>,
    pub max_retries: u32,
    pub retry_backoff_ms: u64,
    /// Hard cap on input + output tokens for one session.
    pub token_budget: u64,
    /// Context characters sent per request; files that do not fit are left
    /// out and reported (never truncated).
    pub context_chars: usize,
    /// Optional USD prices per million tokens, keyed by model id. Prices
    /// change; UMMP ships none and reports tokens when a price is missing.
    pub prices: BTreeMap<String, Price>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

impl Default for AiConfig {
    fn default() -> Self {
        let m = |effort: &str| AgentModel {
            model: "claude-opus-5-5".into(),
            effort: effort.into(),
            max_tokens: 16_000,
        };
        Self {
            agents: BTreeMap::from([
                (AgentKind::Analysis, m("medium")),
                (AgentKind::Migration, m("high")),
                (AgentKind::Debug, m("high")),
                (AgentKind::Review, m("high")),
            ]),
            fallback_model: None,
            max_retries: 2,
            retry_backoff_ms: 1_000,
            token_budget: 400_000,
            context_chars: 200_000,
            prices: BTreeMap::new(),
        }
    }
}

impl AiConfig {
    /// Model settings for an agent (router). Unknown agents get the analysis entry.
    pub fn model_for(&self, agent: AgentKind) -> AgentModel {
        self.agents
            .get(&agent)
            .or_else(|| self.agents.values().next())
            .cloned()
            .unwrap_or_else(|| Self::default().agents[&AgentKind::Analysis].clone())
    }
}

// ---------------------------------------------------------------------------
// Cost tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostTracker {
    pub by_agent: BTreeMap<AgentKind, Usage>,
    pub by_model: BTreeMap<String, Usage>,
}

impl CostTracker {
    pub fn record(&mut self, agent: AgentKind, model: &str, usage: Usage) {
        let a = self.by_agent.entry(agent).or_default();
        a.input_tokens += usage.input_tokens;
        a.output_tokens += usage.output_tokens;
        let m = self.by_model.entry(model.to_string()).or_default();
        m.input_tokens += usage.input_tokens;
        m.output_tokens += usage.output_tokens;
    }

    pub fn total_tokens(&self) -> u64 {
        self.by_agent.values().map(|u| u.total()).sum()
    }

    /// USD cost when every used model has a configured price.
    // Token counts stay far below 2^52, so the f64 conversion is exact.
    #[allow(clippy::cast_precision_loss)]
    pub fn cost_usd(&self, prices: &BTreeMap<String, Price>) -> Option<f64> {
        let mut total = 0.0;
        for (model, u) in &self.by_model {
            let p = prices.get(model)?;
            let input = u.input_tokens as f64 / 1e6 * p.input_per_mtok;
            total += (u.output_tokens as f64 / 1e6).mul_add(p.output_per_mtok, input);
        }
        Some(total)
    }
}

// ---------------------------------------------------------------------------
// Audit trail
// ---------------------------------------------------------------------------

/// One audited AI call or approval decision (a JSON line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub session_id: uuid::Uuid,
    pub event: String,
    pub agent: Option<AgentKind>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub prompt_version: Option<String>,
    pub usage: Option<Usage>,
    pub outcome: String,
    pub prompt: Option<String>,
    pub response: Option<String>,
}

/// Append-only JSONL audit log. `None` path keeps entries in memory only.
#[derive(Debug, Default)]
pub struct AuditLog {
    path: Option<PathBuf>,
    pub entries: Vec<AuditEntry>,
}

impl AuditLog {
    pub fn new(path: Option<&Path>) -> Self {
        Self {
            path: path.map(Path::to_path_buf),
            entries: Vec::new(),
        }
    }

    pub fn record(&mut self, entry: AuditEntry) -> std::io::Result<()> {
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            let line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
            writeln!(f, "{line}")?;
        }
        self.entries.push(entry);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Human approval
// ---------------------------------------------------------------------------

/// Decides, per proposed file change, whether it may be applied.
pub trait ApprovalGate {
    /// `concerns` are the review agent's concerns; `reviewer_approved` is
    /// its verdict. Return `true` to apply this file's change.
    fn approve(
        &mut self,
        path: &Path,
        diff: &str,
        reviewer_approved: bool,
        concerns: &[String],
    ) -> bool;
}

/// Applies nothing (the default when nobody can be asked).
pub struct DenyAll;

impl ApprovalGate for DenyAll {
    fn approve(&mut self, _: &Path, _: &str, _: bool, _: &[String]) -> bool {
        false
    }
}

/// Explicit user policy: apply every change the review agent approved.
/// Changes the reviewer rejected are never applied by this policy.
pub struct ApproveReviewed;

impl ApprovalGate for ApproveReviewed {
    fn approve(&mut self, _: &Path, _: &str, reviewer_approved: bool, _: &[String]) -> bool {
        reviewer_approved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_routes_every_agent() {
        let c = AiConfig::default();
        for a in [
            AgentKind::Analysis,
            AgentKind::Migration,
            AgentKind::Debug,
            AgentKind::Review,
        ] {
            assert_eq!(c.model_for(a).model, "claude-opus-5-5");
        }
        assert_eq!(c.model_for(AgentKind::Analysis).effort, "medium");
        assert_eq!(c.model_for(AgentKind::Migration).effort, "high");
    }

    #[test]
    fn test_config_from_yaml_overrides() {
        let yaml = "agents:\n  analysis: {model: local-small, effort: low, max_tokens: 2000}\ntoken_budget: 1000\n";
        let c: AiConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(c.model_for(AgentKind::Analysis).model, "local-small");
        assert_eq!(c.token_budget, 1000);
        assert_eq!(c.max_retries, 2); // default kept
    }

    #[test]
    fn test_cost_tracker() {
        let mut t = CostTracker::default();
        let u = Usage {
            input_tokens: 1_000_000,
            output_tokens: 100_000,
        };
        t.record(AgentKind::Analysis, "m", u);
        t.record(AgentKind::Review, "m", u);
        assert_eq!(t.total_tokens(), 2_200_000);
        assert_eq!(t.cost_usd(&BTreeMap::new()), None);
        let prices = BTreeMap::from([(
            "m".to_string(),
            Price {
                input_per_mtok: 1.0,
                output_per_mtok: 10.0,
            },
        )]);
        assert!((t.cost_usd(&prices).unwrap() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_audit_log_appends_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit/ai-audit.jsonl");
        let mut log = AuditLog::new(Some(&path));
        for outcome in ["ok", "refused"] {
            log.record(AuditEntry {
                ts: chrono::Utc::now(),
                session_id: uuid::Uuid::nil(),
                event: "call".into(),
                agent: Some(AgentKind::Analysis),
                provider: Some("scripted".into()),
                model: Some("m".into()),
                prompt_version: Some("analysis/1".into()),
                usage: None,
                outcome: outcome.into(),
                prompt: None,
                response: None,
            })
            .unwrap();
        }
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("\"outcome\":\"refused\""));
    }
}
