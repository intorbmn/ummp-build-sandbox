//! Serializable shapes the UI renders. Nothing here carries behaviour.

use crate::adapter::TechFinding;
use crate::jobs::JobState;
use crate::migration::MigrationReport;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub loader: String,
    pub minecraft: String,
    pub java: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisView {
    /// `"folder"`, `"zip"`, or standalone `"jar"` metadata-only view.
    pub kind: String,
    pub mods: Vec<ModView>,
    pub build_system: Option<String>,
    pub languages: Vec<String>,
    pub technologies: Vec<TechFinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingView {
    pub rule: String,
    pub severity: RiskLevel,
    pub file: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskView {
    pub level: Option<RiskLevel>,
    pub findings: Vec<FindingView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetVersion {
    pub minecraft: String,
    pub loaders: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetsView {
    pub loaders: Vec<String>,
    pub versions: Vec<TargetVersion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Modified,
    Created,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiffView {
    pub path: String,
    pub kind: FileKind,
    /// Unified diff text.
    pub diff: String,
    pub replacements: usize,
    /// An adapter finding in this file needs review (assisted or manual handling).
    pub needs_attention: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveView {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedView {
    pub description: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanView {
    pub source_minecraft: String,
    pub target_minecraft: String,
    pub source_loader: Option<String>,
    pub target_loader: String,
    pub files: Vec<FileDiffView>,
    pub moves: Vec<MoveView>,
    pub deletes: Vec<String>,
    pub technologies: Vec<TechFinding>,
    pub skipped: Vec<SkippedView>,
    pub warnings: Vec<String>,
    pub files_scanned: usize,
}

/// What a finished port or migration did, small enough to ride in an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportView {
    pub files_changed: usize,
    pub replacements: usize,
    pub moves: usize,
    pub deletes: usize,
    pub skipped_mappings: usize,
    pub breaking_changes: usize,
    pub warnings: Vec<String>,
    pub technologies: Vec<TechFinding>,
    /// Set for an in-place migration: the checkpoint `rollback` restores.
    pub checkpoint_id: Option<Uuid>,
}

impl From<&MigrationReport> for ReportView {
    fn from(r: &MigrationReport) -> Self {
        Self {
            files_changed: r.changes.len(),
            replacements: r.total_replacements(),
            moves: r.moves.len(),
            deletes: r.deletes.len(),
            skipped_mappings: r.skipped_mappings.len(),
            breaking_changes: r.breaking_changes.len(),
            warnings: r.warnings.clone(),
            technologies: r.technologies.clone(),
            checkpoint_id: r.checkpoint_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressView {
    pub done: u32,
    pub total: u32,
}

/// A job as the UI shows it: state, step, progress, the last log lines, and the outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobView {
    pub id: Uuid,
    pub session: Uuid,
    pub step: String,
    pub state: JobState,
    pub progress: Option<ProgressView>,
    /// The last 200 lines; the whole log is in the job database.
    pub log: Vec<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ---- event payloads (`events::Event.payload`); golden JSON in tests/fixtures/golden/app-events ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobStarted {
    pub job: Uuid,
    pub session: Uuid,
    pub step: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobProgress {
    pub job: Uuid,
    pub step: String,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobLog {
    pub job: Uuid,
    pub line: String,
}

/// One row of "recent sessions" on the Home screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionView {
    pub id: Uuid,
    pub state: JobState,
    /// The last step the job reported.
    pub step: String,
    /// Where the result went (port and pack jobs), when the job succeeded.
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobApproval {
    pub job: Uuid,
    pub approval: Uuid,
    pub path: String,
    pub diff: String,
    pub reviewer_approved: bool,
    pub concerns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobFinished {
    pub job: Uuid,
    pub state: JobState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
