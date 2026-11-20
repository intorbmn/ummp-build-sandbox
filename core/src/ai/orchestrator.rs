//! Orchestrator for the AI agents.
//!
//! Routes each agent to its model, retries transient errors, falls back to a
//! second model, enforces the token budget, audits every call, and runs
//! review → human approval → checkpointed apply.

use super::agents::{
    analysis_prompt, analysis_schema, build_context, debug_prompt, edit_schema, files_mentioned,
    migration_prompt, review_prompt, review_schema, validate_edits, AnalysisOutput, Context,
    EditProposal, Rejected, ReviewOutput, Verdict, ANALYSIS_PROMPT, DEBUG_PROMPT, MIGRATION_PROMPT,
    REVIEW_PROMPT,
};
use super::{
    AgentKind, AiConfig, AiError, AiProvider, AiRequest, ApprovalGate, AuditEntry, AuditLog,
    CostTracker,
};
use crate::checkpoint::{collect_files, CheckpointStore};
use crate::migration::{EditSet, FileEdit, MigrationEngine, MigrationError, RunOptions};
use crate::project::Project;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("ai: {0}")]
    Ai(#[from] AiError),
    #[error("migration: {0}")]
    Migration(#[from] MigrationError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Where approved edits are applied.
pub struct ApplyTarget<'a> {
    pub project: &'a Project,
    pub engine: &'a MigrationEngine,
    pub store: &'a mut CheckpointStore,
    pub snapshot_root: &'a Path,
}

/// Result of an assist or debug session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiSessionReport {
    pub session_id: Uuid,
    pub analysis: Option<AnalysisOutput>,
    pub notes: Vec<String>,
    pub proposed: Vec<PathBuf>,
    pub diffs: Vec<String>,
    pub rejected: Vec<Rejected>,
    pub verdicts: Vec<Verdict>,
    pub applied: Vec<PathBuf>,
    pub checkpoint_id: Option<Uuid>,
    pub omitted_context: Vec<String>,
    pub cost: CostTracker,
}

pub struct Orchestrator<'a> {
    provider: &'a dyn AiProvider,
    pub config: AiConfig,
    pub audit: AuditLog,
    pub cost: CostTracker,
    pub session_id: Uuid,
    /// Sleep between retries (injectable so tests do not wait).
    pub sleep: fn(u64),
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

impl<'a> Orchestrator<'a> {
    pub fn new(provider: &'a dyn AiProvider, config: AiConfig, audit: AuditLog) -> Self {
        Self {
            provider,
            config,
            audit,
            cost: CostTracker::default(),
            session_id: Uuid::new_v4(),
            sleep: sleep_ms,
        }
    }

    fn log(
        &mut self,
        agent: AgentKind,
        version: &str,
        req: &AiRequest,
        outcome: &str,
        usage: Option<super::Usage>,
        response: Option<&str>,
    ) {
        // Audit failures must not abort a session; they surface on the next write.
        let _ = self.audit.record(AuditEntry {
            ts: chrono::Utc::now(),
            session_id: self.session_id,
            event: "call".into(),
            agent: Some(agent),
            provider: Some(self.provider.name().to_string()),
            model: Some(req.model.clone()),
            prompt_version: Some(version.to_string()),
            usage,
            outcome: outcome.to_string(),
            prompt: Some(format!("{}\n\n{}", req.system, req.user)),
            response: response.map(str::to_string),
        });
    }

    /// One agent call with budget, retries, fallback, cost and audit.
    fn call<T: DeserializeOwned>(
        &mut self,
        agent: AgentKind,
        version: &str,
        (system, user): (String, String),
        schema: &serde_json::Value,
    ) -> Result<T, AiError> {
        let routed = self.config.model_for(agent);
        let mut models = vec![routed.model.clone()];
        if let Some(fb) = self.config.fallback_model.clone() {
            if fb != routed.model {
                models.push(fb);
            }
        }
        let mut last = AiError::Fatal("no attempt made".into());
        for (i, model) in models.iter().enumerate() {
            let attempts = if i == 0 {
                self.config.max_retries + 1
            } else {
                1
            };
            for attempt in 0..attempts {
                let spent = self.cost.total_tokens();
                if spent >= self.config.token_budget {
                    return Err(AiError::BudgetExceeded {
                        spent,
                        budget: self.config.token_budget,
                    });
                }
                let req = AiRequest {
                    model: model.clone(),
                    effort: routed.effort.clone(),
                    max_tokens: routed.max_tokens,
                    system: system.clone(),
                    user: user.clone(),
                    schema: schema.clone(),
                };
                match self.provider.complete(&req) {
                    Ok(resp) => {
                        self.cost.record(agent, &resp.model, resp.usage);
                        return match serde_json::from_str::<T>(&resp.text) {
                            Ok(v) => {
                                self.log(
                                    agent,
                                    version,
                                    &req,
                                    "ok",
                                    Some(resp.usage),
                                    Some(&resp.text),
                                );
                                Ok(v)
                            }
                            Err(e) => {
                                self.log(
                                    agent,
                                    version,
                                    &req,
                                    "invalid_output",
                                    Some(resp.usage),
                                    Some(&resp.text),
                                );
                                Err(AiError::InvalidOutput(e.to_string()))
                            }
                        };
                    }
                    Err(AiError::Transient(msg)) => {
                        self.log(
                            agent,
                            version,
                            &req,
                            &format!("transient: {msg}"),
                            None,
                            None,
                        );
                        last = AiError::Transient(msg);
                        if attempt + 1 < attempts {
                            let backoff = self.config.retry_backoff_ms << attempt.min(6);
                            (self.sleep)(backoff);
                        }
                    }
                    Err(e) => {
                        self.log(agent, version, &req, &format!("error: {e}"), None, None);
                        return Err(e);
                    }
                }
            }
        }
        Err(last)
    }

    /// Analysis → Migration → Review → approval → apply.
    pub fn assist(
        &mut self,
        request: &str,
        analysis_text: &str,
        migration_report: &str,
        warnings: &[String],
        approval: &mut dyn ApprovalGate,
        target: &mut ApplyTarget<'_>,
    ) -> Result<AiSessionReport, OrchestratorError> {
        let files = collect_files(&target.project.root)?;
        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.to_str().map(|s| s.replace('\\', "/")))
            .collect();
        let analysis: AnalysisOutput = self.call(
            AgentKind::Analysis,
            ANALYSIS_PROMPT,
            analysis_prompt(analysis_text, migration_report, &names),
            &analysis_schema(),
        )?;
        // Only files that exist in the project can enter the context.
        let candidates: Vec<PathBuf> = analysis
            .files_to_change
            .iter()
            .filter(|f| names.contains(f))
            .map(PathBuf::from)
            .collect();
        let ctx = build_context(target.project, &candidates, self.config.context_chars);
        let proposal: EditProposal = self.call(
            AgentKind::Migration,
            MIGRATION_PROMPT,
            migration_prompt(request, &analysis.risks, warnings, &ctx),
            &edit_schema(),
        )?;
        let mut report = self.finish(request, &proposal, &ctx, approval, target)?;
        report.analysis = Some(analysis);
        Ok(report)
    }

    /// Debug → Review → approval → apply, from a build or crash log.
    pub fn debug(
        &mut self,
        request: &str,
        log: &str,
        approval: &mut dyn ApprovalGate,
        target: &mut ApplyTarget<'_>,
    ) -> Result<AiSessionReport, OrchestratorError> {
        let files = collect_files(&target.project.root)?;
        let candidates = files_mentioned(log, &files);
        let ctx = build_context(target.project, &candidates, self.config.context_chars);
        let proposal: EditProposal = self.call(
            AgentKind::Debug,
            DEBUG_PROMPT,
            debug_prompt(request, log, &ctx),
            &edit_schema(),
        )?;
        self.finish(request, &proposal, &ctx, approval, target)
    }

    fn finish(
        &mut self,
        request: &str,
        proposal: &EditProposal,
        ctx: &Context,
        approval: &mut dyn ApprovalGate,
        target: &mut ApplyTarget<'_>,
    ) -> Result<AiSessionReport, OrchestratorError> {
        let (edits, rejected) = validate_edits(&proposal.edits, ctx);
        let mut report = AiSessionReport {
            session_id: self.session_id,
            notes: proposal.notes.clone(),
            proposed: edits.iter().map(|e| e.path.clone()).collect(),
            diffs: edits.iter().map(FileEdit::unified_diff).collect(),
            rejected,
            omitted_context: ctx.omitted.clone(),
            ..AiSessionReport::default()
        };
        if edits.is_empty() {
            report.cost = self.cost.clone();
            return Ok(report);
        }

        let review: ReviewOutput = self.call(
            AgentKind::Review,
            REVIEW_PROMPT,
            review_prompt(request, &edits),
            &review_schema(),
        )?;
        let mut approved = Vec::new();
        for edit in edits {
            let key = edit.path.to_string_lossy().replace('\\', "/");
            // A file the reviewer did not mention counts as not approved.
            let verdict = review.verdicts.iter().find(|v| v.path == key);
            let reviewer_ok = verdict.is_some_and(|v| v.approve);
            let concerns = verdict.map_or_else(
                || vec!["review agent returned no verdict for this file".into()],
                |v| v.concerns.clone(),
            );
            let yes = approval.approve(&edit.path, &edit.unified_diff(), reviewer_ok, &concerns);
            let _ = self.audit.record(AuditEntry {
                ts: chrono::Utc::now(),
                session_id: self.session_id,
                event: "approval".into(),
                agent: None,
                provider: None,
                model: None,
                prompt_version: None,
                usage: None,
                outcome: format!(
                    "{} {key} (reviewer {})",
                    if yes { "approved" } else { "declined" },
                    if reviewer_ok { "approved" } else { "rejected" }
                ),
                prompt: None,
                response: None,
            });
            if yes {
                approved.push(edit);
            }
        }
        report.verdicts = review.verdicts;

        if !approved.is_empty() {
            let set = EditSet {
                edits: approved,
                ..EditSet::default()
            };
            report.checkpoint_id = target.engine.apply_edit_set(
                &set,
                target.project,
                target.store,
                &RunOptions {
                    dry_run: false,
                    snapshot_root: target.snapshot_root,
                    session_id: self.session_id,
                },
            )?;
            report.applied = set.edits.iter().map(|e| e.path.clone()).collect();
        }
        report.cost = self.cost.clone();
        Ok(report)
    }
}
