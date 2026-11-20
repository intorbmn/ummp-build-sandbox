//! Pack migration and build jobs, with the risk gate in front of them.
//!
//! Mod code is untrusted. A local build runs it on this machine, so a High-risk project is
//! refused unless the person acknowledged the risk, and a Medium-risk one needs a
//! confirmation. A GitHub build runs on a disposable runner and only warns. A pack that
//! contains a High-risk mod is refused by the pack pipeline itself (it writes only the report)
//! unless `allow_risk` says the person reviewed it.

use super::helpers::{mod_identity, open_kb, pack_format, risk_decision, side, RiskDecision};
use super::views::RiskLevel;
use super::{check_input, risk_level, slash, AppError, AppServices, InputKind, Secret};
use crate::build::github::GithubBackend;
use crate::build::local::{default_workdir, LocalBackend};
use crate::build::report::{score, to_markdown};
use crate::build::{BuildBackend, BuildRequest, Stage};
use crate::modpack::plan::ModIndex;
use crate::modpack::run::{migrate_pack, MigrateOptions};
use crate::scan::scan_path;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// A modpack migration job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackRequest {
    pub input: PathBuf,
    pub to: String,
    pub loader: String,
    pub from_loader: Option<String>,
    pub from_mc: Option<String>,
    /// Look mods up online (needs an index that can); offline keeps the plan local.
    pub online: bool,
    pub sources: Option<PathBuf>,
    /// `client`, `server` or `both` (`None` = both).
    pub side: Option<String>,
    /// `None` = `settings.output_root` plus a generated name, else the pipeline default.
    pub output: Option<PathBuf>,
    /// The person reviewed the High-risk mods in this pack and wants it migrated anyway.
    pub allow_risk: bool,
    pub loader_version: Option<String>,
    /// `mrpack`, `curseforge`, `prism`, `folder`; empty = mrpack + folder.
    pub formats: Vec<String>,
    pub min_confidence: f64,
}

impl Default for PackRequest {
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            to: String::new(),
            loader: String::new(),
            from_loader: None,
            from_mc: None,
            online: false,
            sources: None,
            side: None,
            output: None,
            allow_risk: false,
            loader_version: None,
            formats: Vec::new(),
            min_confidence: 0.8,
        }
    }
}

/// A build job on a project folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildJobRequest {
    pub path: PathBuf,
    /// Build on this machine. `false` builds on a GitHub Actions runner.
    pub local: bool,
    /// `owner/name` of the sandbox repository (`None` = `settings.sandbox_repo`, then default).
    pub repo: Option<String>,
    /// Any of `build`, `test`, `server`.
    pub stages: Vec<String>,
    pub timeout_secs: u64,
    pub java: Option<u32>,
    pub workdir: Option<PathBuf>,
    /// The person reviewed the findings and accepts running a High-risk mod on this machine.
    pub accept_high_risk: bool,
    /// The person confirmed a Medium-risk local build.
    pub confirm_medium: bool,
}

impl Default for BuildJobRequest {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            local: false,
            repo: None,
            stages: vec!["build".into(), "test".into(), "server".into()],
            timeout_secs: 1800,
            java: None,
            workdir: None,
            accept_high_risk: false,
            confirm_medium: false,
        }
    }
}

const DEFAULT_SANDBOX_REPO: &str = "intorbmn/ummp-build-sandbox";

fn parse_stages(stages: &[String]) -> Result<Vec<Stage>, AppError> {
    stages
        .iter()
        .map(|s| match s.trim() {
            "build" => Ok(Stage::Build),
            "test" => Ok(Stage::Test),
            "server" => Ok(Stage::Server),
            other => Err(AppError::Input(format!(
                "unknown stage `{other}` (use build, test, server)"
            ))),
        })
        .collect()
}

impl AppServices {
    /// Scans `path` and applies the risk rules for a build. Returns the risk level found.
    ///
    /// # Errors
    /// Returns [`AppError::Refused`] when a High-risk project would run locally without
    /// `accept_high`, or a Medium-risk one without `confirm_medium`, and [`AppError::Input`]
    /// when `path` cannot be scanned.
    pub fn check_build_risk(
        &self,
        path: &Path,
        local: bool,
        accept_high: bool,
        confirm_medium: bool,
    ) -> Result<Option<RiskLevel>, AppError> {
        check_input(path, true)?;
        let scan = scan_path(path).map_err(|e| AppError::Input(e.to_string()))?;
        let risk = scan.risk();
        match risk_decision(risk, local, accept_high, confirm_medium) {
            RiskDecision::Refuse(why) => Err(AppError::Refused(why)),
            RiskDecision::Confirm => Err(AppError::Refused(
                "Medium risk: confirm to build on this machine".into(),
            )),
            RiskDecision::Allow | RiskDecision::AllowWithWarning => Ok(risk.map(risk_level)),
        }
    }

    /// Starts a build job. The risk gate runs first, so a refused project never becomes a job.
    ///
    /// # Errors
    /// Returns [`AppError::Refused`] from the risk gate and [`AppError::Input`] when the path
    /// is not a project folder or a stage name is unknown.
    pub fn start_build(&self, req: BuildJobRequest) -> Result<Uuid, AppError> {
        self.start_build_with_token(req, None)
    }

    /// Starts a build using an optional GitHub token from the caller's OS keyring.
    /// The token is passed only to the trusted GitHub CLI process through GH_TOKEN.
    ///
    /// # Errors
    /// Returns the same errors as AppServices::start_build.
    pub fn start_build_with_token(
        &self,
        req: BuildJobRequest,
        github_token: Option<Secret>,
    ) -> Result<Uuid, AppError> {
        if check_input(&req.path, true)? != InputKind::Folder {
            return Err(AppError::Input(
                "JAR-only mods can only be scanned; building needs the source project".into(),
            ));
        }
        let stages = parse_stages(&req.stages)?;
        let level = self.check_build_risk(
            &req.path,
            req.local,
            req.accept_high_risk,
            req.confirm_medium,
        )?;
        let repo = req
            .repo
            .clone()
            .or_else(|| self.settings().sandbox_repo)
            .unwrap_or_else(|| DEFAULT_SANDBOX_REPO.to_owned());
        self.start_job("build", move |ctx| {
            ctx.step("scan", 1, 3)?;
            if level.is_some() && !req.local {
                ctx.log("risk indicators found; building on a disposable GitHub runner");
            }
            let scan = scan_path(&req.path).map_err(|e| AppError::Input(e.to_string()))?;
            let (mod_name, detected_java) = mod_identity(&req.path);
            let build = BuildRequest {
                project: req.path.clone(),
                mod_name,
                stages,
                java: req.java.or(detected_java),
                timeout: Duration::from_secs(req.timeout_secs),
            };
            let workdir = req.workdir.clone().unwrap_or_else(default_workdir);
            ctx.step("building", 2, 3)?;
            let outcome = if req.local {
                LocalBackend {
                    workdir,
                    gradle: None,
                }
                .run(&build)
            } else {
                ctx.log(&format!("building on GitHub Actions in {repo}"));
                GithubBackend {
                    repo,
                    gh: "gh".into(),
                    git: "git".into(),
                    poll: Duration::from_secs(15),
                    workdir,
                    token: github_token,
                }
                .run(&build)
            }
            .map_err(|e| AppError::Input(e.to_string()))?;
            let md = to_markdown(&outcome, &scan);
            std::fs::write(outcome.workdir.join("ummp-report.md"), &md)?;
            let (passed, run) = score(&outcome);
            ctx.progress("done", 3, 3);
            Ok(serde_json::json!({
                "passed": passed == run,
                "stages_passed": passed,
                "stages_run": run,
                "workdir": slash(&outcome.workdir),
                "report": md,
            }))
        })
    }

    /// Starts a modpack migration job. A pack with a High-risk mod fails with a "risk" error
    /// (and a report, no export) unless `allow_risk` is set.
    ///
    /// # Errors
    /// Returns [`AppError::Input`] for an unknown side or format, or a missing input.
    pub fn start_pack(
        &self,
        req: PackRequest,
        index: Arc<dyn ModIndex + Send + Sync>,
    ) -> Result<Uuid, AppError> {
        check_input(&req.input, false)?;
        let formats = req
            .formats
            .iter()
            .map(|f| pack_format(f))
            .collect::<Result<Vec<_>, _>>()?;
        let side = side(req.side.as_deref().unwrap_or("both"))?;
        let settings = self.settings();
        let output = req.output.clone().or_else(|| {
            settings
                .output_root
                .as_ref()
                .map(|root| root.join(default_pack_name(&req)))
        });
        let kb_path = settings.kb_path;
        self.start_job("pack", move |ctx| {
            ctx.step("validate", 1, 3)?;
            let kb = open_kb(kb_path, None)?;
            ctx.step("migrating", 2, 3)?;
            let outcome = migrate_pack(
                &MigrateOptions {
                    input: req.input,
                    to: req.to,
                    loader: req.loader,
                    from_loader: req.from_loader,
                    from_mc: req.from_mc,
                    online: req.online,
                    sources: req.sources,
                    side,
                    output,
                    allow_risk: req.allow_risk,
                    loader_version: req.loader_version,
                    formats,
                    threads: 4,
                    min_confidence: req.min_confidence,
                },
                index.as_ref(),
                &kb,
            )?;
            if let Some(why) = &outcome.plan.refused {
                ctx.log(&format!("refused: {why}"));
                return Err(AppError::Refused(format!(
                    "pack refused because of risk: {why}. Review the findings, then \
                     acknowledge the risk to continue"
                )));
            }
            ctx.progress("done", 3, 3);
            let output = slash(&outcome.output);
            let written = outcome.written.iter().map(|p| slash(p)).collect::<Vec<_>>();
            let manual = outcome.plan.manual_count();
            let exit_code = outcome.exit_code();
            let items = outcome.plan.items;
            Ok(serde_json::json!({
                "output": output,
                "written": written,
                "manual": manual,
                "items": items,
                "exit_code": exit_code,
            }))
        })
    }
}

/// A folder name for a pack written under `settings.output_root`.
fn default_pack_name(req: &PackRequest) -> String {
    let stem = req
        .input
        .file_stem()
        .map_or_else(|| "pack".to_owned(), |s| s.to_string_lossy().into_owned());
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    };
    format!(
        "{}-{}-{}-{}",
        clean(&stem),
        clean(&req.to),
        clean(&req.loader),
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    )
}
