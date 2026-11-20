//! The AI assist job and its approval gate.
//!
//! The orchestrator asks, per file, whether a change may be applied (`ApprovalGate`). Here the
//! answer comes from the person: the job publishes `job.approval` and waits until the window
//! calls [`AppServices::decide`]. Cancel, a closed channel, or anything but an explicit yes
//! means "do not write". Edits still reach disk only through `MigrationEngine` after a
//! checkpoint (ADR 0005/0006).

use super::jobs::{CancelToken, Shared};
use super::snapshots::{default_snapshots, load_store, save_store};
use super::views::JobApproval;
use super::{
    check_input, input_err, resolve_plan, slash, AppError, AppServices, InputKind, PlanRequest,
    EVENT_JOB_APPROVAL,
};
use crate::ai::orchestrator::{ApplyTarget, Orchestrator};
use crate::ai::{AiConfig, AiProvider, ApprovalGate, AuditLog};
use crate::analyzer::AnalyzerEngine;
use crate::filesystem::{FileSystem, LocalFileSystem};
use crate::migration::{MigrationEngine, RunOptions};
use crate::project::{DefaultProjectManager, ProjectManager};
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// How often a waiting gate looks at the cancel flag.
const GATE_POLL: Duration = Duration::from_millis(100);

/// Asks the person through the event bus and waits for [`AppServices::decide`].
pub struct ChannelGate {
    shared: Arc<Shared>,
    job: Uuid,
    cancel: CancelToken,
}

impl ApprovalGate for ChannelGate {
    fn approve(
        &mut self,
        path: &Path,
        diff: &str,
        reviewer_approved: bool,
        concerns: &[String],
    ) -> bool {
        let approval = Uuid::new_v4();
        let (tx, rx) = mpsc::channel::<bool>();
        self.shared
            .approvals
            .lock()
            .expect("approvals lock")
            .insert(approval, tx);
        let payload = JobApproval {
            job: self.job,
            approval,
            path: slash(path),
            diff: diff.to_owned(),
            reviewer_approved,
            concerns: concerns.to_vec(),
        };
        self.shared.bus.publish(
            EVENT_JOB_APPROVAL,
            serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null),
        );
        let answer = loop {
            match rx.recv_timeout(GATE_POLL) {
                Ok(yes) => break yes,
                Err(RecvTimeoutError::Timeout) => {
                    if self.cancel.is_cancelled() {
                        break false;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break false,
            }
        };
        self.shared
            .approvals
            .lock()
            .expect("approvals lock")
            .remove(&approval);
        answer
    }
}

impl AppServices {
    /// Answers a pending `job.approval` question: `true` applies that file's change.
    ///
    /// # Errors
    /// Returns [`AppError::NotFound`] when the question is unknown or was already answered.
    pub fn decide(&self, approval: Uuid, approve: bool) -> Result<(), AppError> {
        let tx = self
            .shared
            .approvals
            .lock()
            .expect("approvals lock")
            .get(&approval)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("approval {approval}")))?;
        tx.send(approve)
            .map_err(|_| AppError::NotFound(format!("approval {approval}")))
    }

    /// Starts an AI assist job on a project folder. The model only proposes edits; each file
    /// waits for the person's decision before anything is written. The provider is passed in
    /// because the HTTP clients live outside this crate.
    ///
    /// # Errors
    /// Returns [`AppError::Input`] when the input is missing or not a folder.
    pub fn start_ai_assist(
        &self,
        req: PlanRequest,
        provider: Arc<dyn AiProvider>,
        config: AiConfig,
    ) -> Result<Uuid, AppError> {
        if check_input(&req.input, false)? != InputKind::Folder {
            return Err(AppError::Input(
                "AI assist needs a mod folder, not a .zip file".into(),
            ));
        }
        let kb_path = self.settings().kb_path;
        let shared = Arc::clone(&self.shared);
        self.start_job("ai-assist", move |ctx| {
            ctx.step("validate", 1, 4)?;
            let project = DefaultProjectManager.open(&req.input).map_err(input_err)?;
            ctx.step("analysing", 2, 4)?;
            let resolved = resolve_plan(kb_path, &project, &req)?;
            let snap_dir = default_snapshots(&project.root);
            let mut store = load_store(&snap_dir)?;
            let engine = MigrationEngine::default();
            // The rule-based dry run and the analysis are the agents' input.
            let rules = engine.run(
                &resolved.plan,
                &project,
                &mut store,
                &RunOptions {
                    dry_run: true,
                    snapshot_root: &snap_dir,
                    session_id: ctx.id(),
                },
            )?;
            let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
            let analysis = AnalyzerEngine::default()
                .analyze(&project.root, &fs)
                .map_err(input_err)?;
            ctx.step("assisting", 3, 4)?;
            let mut orchestrator = Orchestrator::new(
                provider.as_ref(),
                config,
                AuditLog::new(Some(&snap_dir.join("ai-audit.jsonl"))),
            );
            let mut gate = ChannelGate {
                shared,
                job: ctx.id(),
                cancel: ctx.cancel_token(),
            };
            let report = orchestrator.assist(
                &format!(
                    "Minecraft {} -> {} on {}",
                    resolved.from, req.to, resolved.target_loader
                ),
                &analysis.to_text(),
                &rules.to_markdown(),
                &rules.warnings,
                &mut gate,
                &mut ApplyTarget {
                    project: &project,
                    engine: &engine,
                    store: &mut store,
                    snapshot_root: &snap_dir,
                },
            )?;
            if report.checkpoint_id.is_some() {
                save_store(&snap_dir, &store)?;
            }
            if ctx.check_cancelled().is_err() {
                // A cancel that arrived after some files were already approved and applied.
                if let Some(id) = report.checkpoint_id {
                    store.restore(id, &project.root)?;
                    ctx.log("cancelled: the folder was restored from the checkpoint");
                }
                return Err(AppError::Cancelled);
            }
            ctx.progress("done", 4, 4);
            Ok(serde_json::json!({
                "proposed": report.proposed.iter().map(|p| slash(p)).collect::<Vec<_>>(),
                "applied": report.applied.iter().map(|p| slash(p)).collect::<Vec<_>>(),
                "rejected": report.rejected.len(),
                "checkpoint": report.checkpoint_id,
            }))
        })
    }
}
