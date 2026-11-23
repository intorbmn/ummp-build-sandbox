//! Application services: every action the desktop UI (and the CLI) can take, as plain
//! Rust functions. The UI crate only forwards to these (ADR 0004); it holds no logic.

pub mod approval;
pub mod credentials;
pub mod helpers;
pub mod jobs;
pub mod runs;
pub mod snapshots;
pub mod views;

use crate::adapter::{loader, Handling};
use crate::ai::orchestrator::OrchestratorError;
use crate::analyzer::{inspect_jar, AnalyzerEngine};
use crate::checkpoint::CheckpointError;
use crate::filesystem::{FileSystem, LocalFileSystem};
use crate::migration::{
    MigrationEngine, MigrationError, MigrationPlan, MigrationPlanner, MigrationRequest, RunOptions,
};
use crate::modpack::PackError;
use crate::persistence::{DbError, JobDb};
use crate::port::{default_port_root, port, stage_source, PortError, PortOptions};
use crate::project::{DefaultProjectManager, Project, ProjectManager};
use crate::scan::{scan_path, Severity};
use helpers::{check_input, loader_label, open_kb, InputKind};
use serde::{Deserialize, Serialize};
use snapshots::{default_snapshots, load_store, save_store};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use views::{
    AnalysisView, FileDiffView, FileKind, FindingView, ModView, MoveView, PlanView, ReportView,
    RiskLevel, RiskView, SkippedView, TargetVersion, TargetsView,
};

/// A key or token held in memory only. Not `Serialize`; prints as `***`.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The raw value. Call this only where it is handed to a provider or backend.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The person's input is wrong; the message says what to choose instead.
    #[error("{0}")]
    Input(String),
    /// A safety rule stopped the action.
    #[error("{0}")]
    Refused(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error(transparent)]
    Pack(#[from] PackError),
    #[error(transparent)]
    Migration(#[from] MigrationError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Checkpoint(#[from] CheckpointError),
    #[error(transparent)]
    Ai(#[from] OrchestratorError),
    #[error("cancelled")]
    Cancelled,
    #[error("not found: {0}")]
    NotFound(String),
}

/// Stored in `settings.json`. Holds no keys or tokens: those are passed per call as [`Secret`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Holds `jobs.db` and `settings.json`. Always the folder given to [`AppServices::open`].
    pub data_dir: PathBuf,
    /// `None` = built-in mappings only (in-memory knowledge base).
    pub kb_path: Option<PathBuf>,
    /// `None` = the default port and pack output roots.
    pub output_root: Option<PathBuf>,
    pub sandbox_repo: Option<String>,
    /// `"anthropic"` or `"openai"`.
    pub ai_provider: Option<String>,
    /// `"vi"` or `"en"`.
    pub language: String,
}

impl Settings {
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            kb_path: None,
            output_root: None,
            sandbox_repo: None,
            ai_provider: None,
            language: "en".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanRequest {
    pub input: PathBuf,
    /// `None` = detect from the descriptor.
    pub from: Option<String>,
    pub to: String,
    /// `None` = keep the current loader.
    pub loader: Option<String>,
    pub min_confidence: f64,
    /// Optional source loader override; otherwise detected from the mod descriptor.
    #[serde(default)]
    pub source_loader: Option<String>,
    /// Optional command-level knowledge base and mappings overrides.
    #[serde(default)]
    pub kb: Option<PathBuf>,
    #[serde(default)]
    pub mappings: Option<PathBuf>,
    /// Optional checkpoint directory for in-place migration.
    #[serde(default)]
    pub snapshots: Option<PathBuf>,
}

/// Event names the shell forwards to the window (the payloads are in [`views`]).
pub const EVENT_JOB_STARTED: &str = "job.started";
pub const EVENT_JOB_PROGRESS: &str = "job.progress";
pub const EVENT_JOB_LOG: &str = "job.log";
pub const EVENT_JOB_APPROVAL: &str = "job.approval";
pub const EVENT_JOB_FINISHED: &str = "job.finished";

/// A port job: the source goes in, a ported copy comes out; the source is never written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortRequest {
    pub input: PathBuf,
    /// `None` = `settings.output_root` (or the default port root) plus a generated name.
    pub output: Option<PathBuf>,
    pub to: String,
    /// `None` = detect from the descriptor.
    pub from: Option<String>,
    /// `None` = keep the current loader.
    pub loader: Option<String>,
    pub zip: bool,
    pub min_confidence: f64,
    /// Optional command-level overrides for the configured knowledge base.
    #[serde(default)]
    pub kb: Option<PathBuf>,
    #[serde(default)]
    pub mappings: Option<PathBuf>,
}

pub use runs::{BuildJobRequest, PackRequest};

const SETTINGS_FILE: &str = "settings.json";
const JOBS_FILE: &str = "jobs.db";

pub struct AppServices {
    settings: Mutex<Settings>,
    shared: Arc<jobs::Shared>,
}

impl AppServices {
    /// Opens the services on `settings.data_dir`, creating it, and applies any saved
    /// `settings.json` over the defaults.
    ///
    /// # Errors
    /// Returns an error when the data folder cannot be created or `settings.json` is invalid.
    pub fn open(settings: Settings) -> Result<Self, AppError> {
        std::fs::create_dir_all(&settings.data_dir)?;
        let mut current = read_settings(&settings.data_dir)?.unwrap_or_else(|| settings.clone());
        current.data_dir = settings.data_dir;
        let shared = Arc::new(jobs::Shared::new(JobDb::open(
            &current.data_dir.join(JOBS_FILE),
        )?));
        shared.fail_stale_jobs()?;
        Ok(Self {
            settings: Mutex::new(current),
            shared,
        })
    }

    #[must_use]
    pub fn settings(&self) -> Settings {
        self.settings.lock().expect("settings lock").clone()
    }

    /// Validates and saves the settings (`data_dir` never changes).
    ///
    /// # Errors
    /// Returns [`AppError::Input`] for an unknown language, or an I/O error.
    pub fn save_settings(&self, mut s: Settings) -> Result<(), AppError> {
        if !matches!(s.language.as_str(), "vi" | "en") {
            return Err(AppError::Input(format!(
                "unknown language {:?} (vi, en)",
                s.language
            )));
        }
        let mut guard = self.settings.lock().expect("settings lock");
        s.data_dir.clone_from(&guard.data_dir);
        write_settings(&s)?;
        *guard = s;
        drop(guard);
        Ok(())
    }

    /// What UMMP finds in a mod source folder or `.zip`. Writes nothing.
    ///
    /// # Errors
    /// Returns [`AppError::Input`] when the path is wrong or holds no mod.
    pub fn analyze(&self, input: &Path) -> Result<AnalysisView, AppError> {
        let kind = check_input(input, true)?;
        if kind == InputKind::Jar {
            let m = inspect_jar(input, &LocalFileSystem).map_err(input_err)?;
            return Ok(AnalysisView {
                kind: "jar".into(),
                mods: vec![ModView {
                    id: m.metadata.mod_id,
                    name: m.metadata.name,
                    version: m.metadata.version,
                    loader: loader_label(&m.detected_loader)
                        .map_or_else(|| format!("{:?}", m.detected_loader), str::to_owned),
                    minecraft: m.metadata.minecraft.to_string(),
                    java: m.detected_java.map(|j| j.major),
                }],
                build_system: None,
                languages: Vec::new(),
                technologies: Vec::new(),
            });
        }
        let staging = tempfile::tempdir()?;
        let root = stage_source(input, staging.path())?;
        let project = DefaultProjectManager.open(&root).map_err(input_err)?;
        let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
        let report = AnalyzerEngine::default()
            .analyze(&project.root, &fs)
            .map_err(input_err)?;
        if report.mods.is_empty() {
            return Err(AppError::Input(format!(
                "no mod found in {}. Choose the mod's source folder (it contains the descriptor, e.g. fabric.mod.json).",
                input.display()
            )));
        }
        Ok(AnalysisView {
            kind: if kind == InputKind::Zip {
                "zip"
            } else {
                "folder"
            }
            .into(),
            mods: report
                .mods
                .iter()
                .map(|m| ModView {
                    id: m.metadata.mod_id.clone(),
                    name: m.metadata.name.clone(),
                    version: m.metadata.version.clone(),
                    loader: loader_label(&m.metadata.loader)
                        .map_or_else(|| format!("{:?}", m.metadata.loader), str::to_owned),
                    minecraft: m.metadata.minecraft.to_string(),
                    java: m.detected_java.as_ref().map(|j| j.major),
                })
                .collect(),
            build_system: report.build_system.map(|b| format!("{:?}", b.kind)),
            languages: report.languages,
            technologies: report.technologies,
        })
    }

    /// Malware-risk indicators in a folder, `.zip` or `.jar`. Never runs anything.
    ///
    /// # Errors
    /// Returns [`AppError::Input`] when the path is wrong or unreadable.
    pub fn scan(&self, input: &Path) -> Result<RiskView, AppError> {
        check_input(input, true)?;
        let report = scan_path(input).map_err(input_err)?;
        Ok(RiskView {
            level: report.risk().map(risk_level),
            findings: report
                .findings
                .iter()
                .map(|f| FindingView {
                    rule: f.rule.clone(),
                    severity: risk_level(f.severity),
                    file: f.file.clone(),
                    evidence: f.evidence.clone(),
                })
                .collect(),
        })
    }

    /// Every loader the converter knows, and the Minecraft versions UMMP ships build
    /// templates for (ports to other versions stay possible through `plan`).
    #[must_use]
    pub fn targets(&self) -> TargetsView {
        let mut versions: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        for (l, mc) in loader::templates() {
            if !versions.contains_key(mc) {
                order.push(mc.to_owned());
            }
            let entry = versions.entry(mc.to_owned()).or_default();
            if !entry.iter().any(|x| x == l) {
                entry.push(l.to_owned());
            }
        }
        TargetsView {
            loaders: ["forge", "fabric", "neoforge", "quilt"]
                .map(str::to_owned)
                .to_vec(),
            versions: order
                .into_iter()
                .map(|mc| TargetVersion {
                    loaders: versions.remove(&mc).unwrap_or_default(),
                    minecraft: mc,
                })
                .collect(),
        }
    }

    /// The edits a migration would make, as diffs. Never writes.
    ///
    /// # Errors
    /// Returns [`AppError::Input`] for bad input or an undetectable version or loader, and
    /// [`AppError::Migration`] when the planner refuses (for example a downgrade).
    pub fn plan(&self, req: &PlanRequest) -> Result<PlanView, AppError> {
        check_input(&req.input, false)?;
        let staging = tempfile::tempdir()?;
        let root = stage_source(&req.input, staging.path())?;
        let project = DefaultProjectManager.open(&root).map_err(input_err)?;
        let Resolved {
            plan,
            from,
            source_loader,
            target_loader,
        } = resolve_plan(
            plan_kb_path(&self.settings(), req),
            req.mappings.clone(),
            &project,
            req,
        )?;
        let set = MigrationEngine::default().compute_edits(&plan, &project)?;
        let attention: Vec<String> = set
            .technologies
            .iter()
            .filter(|t| t.handling != Handling::Auto)
            .map(|t| t.file.replace('\\', "/"))
            .collect();
        Ok(PlanView {
            source_minecraft: from,
            target_minecraft: req.to.clone(),
            source_loader,
            target_loader,
            files: set
                .edits
                .iter()
                .map(|e| {
                    let path = slash(&e.path);
                    FileDiffView {
                        kind: if e.original.is_empty() {
                            FileKind::Created
                        } else {
                            FileKind::Modified
                        },
                        diff: e.unified_diff(),
                        replacements: e.replacements.len(),
                        needs_attention: attention.iter().any(|a| *a == path),
                        path,
                    }
                })
                .collect(),
            moves: set
                .moves
                .iter()
                .map(|m| MoveView {
                    from: slash(&m.from),
                    to: slash(&m.to),
                })
                .collect(),
            deletes: set.deletes.iter().map(|p| slash(p)).collect(),
            technologies: set.technologies,
            skipped: plan
                .skipped
                .iter()
                .map(|s| SkippedView {
                    description: format!("{} -> {}", s.mapping.from_api, s.mapping.to_api),
                    reason: s.reason.clone(),
                })
                .collect(),
            warnings: set.warnings,
            files_scanned: set.files_scanned,
        })
    }
}

fn read_settings(data_dir: &Path) -> Result<Option<Settings>, AppError> {
    let file = data_dir.join(SETTINGS_FILE);
    if !file.exists() {
        return Ok(None);
    }
    serde_json::from_slice(&std::fs::read(&file)?)
        .map(Some)
        .map_err(|e| AppError::Input(format!("{} is not valid: {e}", file.display())))
}

fn write_settings(s: &Settings) -> Result<(), AppError> {
    let json = serde_json::to_vec_pretty(s)
        .map_err(|e| AppError::Input(format!("cannot write settings: {e}")))?;
    let tmp = s.data_dir.join("settings.json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, s.data_dir.join(SETTINGS_FILE))?;
    Ok(())
}

/// Wraps an engine error that has no variant of its own as a message for the person.
/// A migration plan plus the versions and loaders it was resolved from.
struct Resolved {
    plan: MigrationPlan,
    from: String,
    source_loader: Option<String>,
    target_loader: String,
}

/// Detects what the request leaves open (source version, loaders) and plans the migration.
fn resolve_plan(
    kb_path: Option<PathBuf>,
    mappings: Option<PathBuf>,
    project: &Project,
    req: &PlanRequest,
) -> Result<Resolved, AppError> {
    let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
    let report = AnalyzerEngine::default()
        .analyze(&project.root, &fs)
        .map_err(input_err)?;
    let first = report.mods.first();
    let source_loader = req
        .source_loader
        .as_deref()
        .map(str::to_ascii_lowercase)
        .or_else(|| first.and_then(|m| loader_label(&m.metadata.loader).map(str::to_owned)));
    let from = match (&req.from, first) {
        (Some(f), _) => f.clone(),
        (None, Some(m)) => m.metadata.minecraft.to_string(),
        (None, None) => {
            return Err(AppError::Input(
                "cannot detect the Minecraft version: choose the source version".into(),
            ))
        }
    };
    let target_loader = match (&req.loader, &source_loader) {
        (Some(l), _) => l.to_ascii_lowercase(),
        (None, Some(l)) => l.clone(),
        (None, None) => {
            return Err(AppError::Input(
                "cannot detect the loader: choose a target loader".into(),
            ))
        }
    };
    let kb = open_kb(kb_path, mappings)?;
    let request = MigrationRequest {
        source_minecraft: from.clone(),
        target_minecraft: req.to.clone(),
        target_loader: target_loader.clone(),
        min_confidence: req.min_confidence,
        source_loader: source_loader.clone(),
    };
    let plan = MigrationPlanner::plan(&kb, &request)?;
    Ok(Resolved {
        plan,
        from,
        source_loader,
        target_loader,
    })
}

/// Selects request-level KB overrides before the saved application setting.
fn plan_kb_path(settings: &Settings, req: &PlanRequest) -> Option<PathBuf> {
    req.kb.clone().or_else(|| {
        if req.mappings.is_none() {
            settings.kb_path.clone()
        } else {
            None
        }
    })
}

/// Canonical form of `p`, or of its nearest existing ancestor with the rest appended.
fn resolve_loose(p: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut cur = p;
    loop {
        if let Ok(c) = cur.canonicalize() {
            return rest.iter().rev().fold(c, |acc, n| acc.join(n));
        }
        match (cur.parent(), cur.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                cur = parent;
            }
            _ => return p.to_path_buf(),
        }
    }
}

/// `{stem}-{to}-{loader}-{timestamp}`, safe as a single folder name.
fn default_output_name(input: &Path, to: &str, loader: &str) -> String {
    let stem = input
        .file_stem()
        .map_or_else(|| "mod".to_owned(), |s| s.to_string_lossy().into_owned());
    let clean = |s: &str| {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
    };
    format!(
        "{}-{}-{}-{}",
        clean(&stem),
        clean(to),
        clean(loader),
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    )
}

fn input_err(e: impl std::fmt::Display) -> AppError {
    AppError::Input(e.to_string())
}

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

const fn risk_level(s: Severity) -> RiskLevel {
    match s {
        Severity::Low => RiskLevel::Low,
        Severity::Medium => RiskLevel::Medium,
        Severity::High => RiskLevel::High,
    }
}

impl AppServices {
    /// Starts a port job and returns its id at once. Cheap checks run before the job starts,
    /// so a bad input or output is an error here and not a failed job.
    ///
    /// # Errors
    /// Returns [`AppError::Input`] when the input is missing, the output folder is not empty
    /// or lies inside the source folder.
    pub fn start_port(&self, req: PortRequest) -> Result<Uuid, AppError> {
        check_input(&req.input, false)?;
        let settings = self.settings();
        let output = req.output.clone().unwrap_or_else(|| {
            let root = settings
                .output_root
                .clone()
                .unwrap_or_else(default_port_root);
            let loader = req.loader.as_deref().unwrap_or("same");
            root.join(default_output_name(&req.input, &req.to, loader))
        });
        if output.exists() && std::fs::read_dir(&output)?.next().is_some() {
            return Err(AppError::Input(format!(
                "output folder {} is not empty",
                output.display()
            )));
        }
        if req.input.is_dir() && resolve_loose(&output).starts_with(resolve_loose(&req.input)) {
            return Err(AppError::Input(format!(
                "output folder {} is inside the source folder",
                output.display()
            )));
        }
        let kb_path = req.kb.clone().or_else(|| {
            if req.mappings.is_none() {
                settings.kb_path
            } else {
                None
            }
        });
        let mappings = req.mappings.clone();
        self.start_job("port", move |ctx| {
            ctx.step("validate", 1, 3)?;
            let kb = open_kb(kb_path, mappings)?;
            ctx.step("porting", 2, 3)?;
            let result = port(
                &PortOptions {
                    input: req.input,
                    output: Some(output),
                    to: req.to,
                    loader: req.loader,
                    from: req.from,
                    zip: req.zip,
                    min_confidence: req.min_confidence,
                },
                &kb,
            )?;
            ctx.log(&format!("wrote {}", result.output.display()));
            ctx.progress("done", 3, 3);
            Ok(serde_json::json!({
                "output": slash(&result.output),
                "zip": result.zip.as_deref().map(slash),
                "report": ReportView::from(&result.report),
            }))
        })
    }
}

impl AppServices {
    /// Starts an in-place migration of a project folder as a job. The folder is only touched
    /// after the person reviewed the plan (`reviewed`) and a checkpoint was taken; cancelling
    /// at any step restores the checkpoint.
    ///
    /// # Errors
    /// Returns [`AppError::Refused`] when the plan was not reviewed and [`AppError::Input`]
    /// when the input is missing or not a folder. Nothing is written in either case.
    pub fn start_migrate_in_place(
        &self,
        req: PlanRequest,
        reviewed: bool,
    ) -> Result<Uuid, AppError> {
        if !reviewed {
            return Err(AppError::Refused(
                "review the plan before changing the folder in place".into(),
            ));
        }
        if check_input(&req.input, false)? != InputKind::Folder {
            return Err(AppError::Input(
                "in-place migration needs a mod folder, not a .zip file".into(),
            ));
        }
        let settings = self.settings();
        let kb_path = plan_kb_path(&settings, &req);
        let mappings = req.mappings.clone();
        let snapshots = req.snapshots.clone();
        self.start_job("migrate", move |ctx| {
            ctx.step("validate", 1, 5)?;
            let project = DefaultProjectManager.open(&req.input).map_err(input_err)?;
            ctx.step("planning", 2, 5)?;
            let resolved = resolve_plan(kb_path, mappings, &project, &req)?;
            ctx.step("applying", 3, 5)?;
            let snap_dir = snapshots.unwrap_or_else(|| default_snapshots(&project.root));
            let mut store = load_store(&snap_dir)?;
            let report = MigrationEngine::default().run(
                &resolved.plan,
                &project,
                &mut store,
                &RunOptions {
                    dry_run: false,
                    snapshot_root: &snap_dir,
                    session_id: ctx.id(),
                },
            )?;
            save_store(&snap_dir, &store)?;
            if let Err(cancelled) = ctx.step("verify", 4, 5) {
                if let Some(id) = report.checkpoint_id {
                    store.restore(id, &project.root)?;
                    ctx.log("cancelled: the folder was restored from the checkpoint");
                }
                return Err(cancelled);
            }
            ctx.progress("done", 5, 5);
            Ok(serde_json::json!({
                "checkpoint": report.checkpoint_id,
                "report": ReportView::from(&report),
            }))
        })
    }

    /// Restores `project` from a checkpoint (the latest when `checkpoint` is `None`).
    ///
    /// # Errors
    /// Returns [`AppError::Input`] when the project cannot be opened or has no checkpoint,
    /// and [`AppError::Checkpoint`] when the restore itself fails.
    pub fn rollback(&self, project: &Path, checkpoint: Option<Uuid>) -> Result<(), AppError> {
        self.rollback_with_snapshots(project, checkpoint, None)
            .map(|_| ())
    }

    /// Restores a checkpoint from the supplied snapshot directory, or the project default.
    ///
    /// # Errors
    /// Returns [`AppError::Input`] when the project or checkpoint is missing, and
    /// [`AppError::Checkpoint`] when the restore itself fails.
    pub fn rollback_with_snapshots(
        &self,
        project: &Path,
        checkpoint: Option<Uuid>,
        snapshots: Option<&Path>,
    ) -> Result<Uuid, AppError> {
        let project = DefaultProjectManager.open(project).map_err(input_err)?;
        let snap_dir =
            snapshots.map_or_else(|| default_snapshots(&project.root), Path::to_path_buf);
        let store = load_store(&snap_dir)?;
        let id = match checkpoint {
            Some(id) => id,
            None => {
                store
                    .latest()
                    .ok_or_else(|| AppError::Input("no checkpoints found for this project".into()))?
                    .id
            }
        };
        store.restore(id, &project.root)?;
        Ok(id)
    }
}
