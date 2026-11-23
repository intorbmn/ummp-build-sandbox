//! Out-of-process plugin protocol (Phase 12.E).
use crate::adapter::{AdapterContext, Handling, Level, TechAdapter, TechFinding};
use crate::build::process::{run_with_input, scrubbed_env, ProcSpec, ProcStatus};
use crate::filesystem::resolve_within;
use crate::migration::{EditSet, FileEdit, MigrationError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PLUGIN_MANIFEST: &str = "ummp-plugin.toml";
pub const PLUGIN_API: u32 = 1;
pub const PLUGIN_TIMEOUT: Duration = Duration::from_secs(30);
pub const PLUGIN_STDOUT_LIMIT: usize = 8 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    Adapter,
    Rules,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub kind: PluginKind,
    pub command: Vec<String>,
    pub ummp_api: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub kind: PluginKind,
    pub ummp_api: u32,
    pub directory: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid plugin manifest: {0}")]
    Manifest(String),
    #[error("plugin protocol: {0}")]
    Protocol(String),
    #[error("plugin API {found} is incompatible; UMMP supports API {supported}")]
    Api { found: u32, supported: u32 },
    #[error("plugin timed out after {0}s")]
    Timeout(u64),
    #[error("plugin stdout exceeded {0} bytes")]
    OutputLimit(usize),
    #[error("plugin request exceeded {0} bytes")]
    RequestLimit(usize),
}

#[derive(Serialize)]
struct PluginFile {
    path: String,
    content: String,
}
#[derive(Deserialize)]
struct PluginEdit {
    path: PathBuf,
    original: String,
    updated: String,
}

pub struct PluginManager {
    root: PathBuf,
}

impl PluginManager {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_dir() -> Result<PathBuf, crate::app::AppError> {
        Ok(crate::app::credentials::default_data_dir()?.join("plugins"))
    }

    pub fn list(&self) -> Result<Vec<PluginInfo>, PluginError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                out.push(PluginAdapter::load(&entry.path())?.info());
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub fn check(&self, directory: &Path) -> Result<PluginInfo, PluginError> {
        let plugin = PluginAdapter::load(directory)?;
        plugin.hello()?;
        Ok(plugin.info())
    }

    pub fn adapters(&self) -> Result<Vec<Box<dyn TechAdapter>>, PluginError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut out: Vec<Box<dyn TechAdapter>> = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                out.push(Box::new(PluginAdapter::load(&entry.path())?));
            }
        }
        Ok(out)
    }
}

/// Decode the plugin's single-line JSON response without starting a child process.
///
/// The output cap is checked here as well as by the process runner so callers that
/// validate protocol bytes directly (including fuzzers) get the same bound.
pub fn decode_plugin_response(output: &[u8]) -> Result<Value, PluginError> {
    if output.len() > PLUGIN_STDOUT_LIMIT {
        return Err(PluginError::OutputLimit(PLUGIN_STDOUT_LIMIT));
    }
    let text = std::str::from_utf8(output)
        .map_err(|_| PluginError::Protocol("response is not UTF-8".into()))?;
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let line = lines
        .next()
        .ok_or_else(|| PluginError::Protocol("plugin returned no response".into()))?;
    if lines.next().is_some() {
        return Err(PluginError::Protocol(
            "expected exactly one JSON response line".into(),
        ));
    }
    serde_json::from_str(line).map_err(|e| PluginError::Protocol(e.to_string()))
}

#[derive(Debug)]
struct PluginAdapter {
    info: PluginInfo,
    program: PathBuf,
    args: Vec<String>,
}

impl PluginAdapter {
    fn load(directory: &Path) -> Result<Self, PluginError> {
        let directory = directory
            .canonicalize()
            .map_err(|e| PluginError::Manifest(format!("plugin directory unavailable: {e}")))?;
        let manifest_path = directory.join(PLUGIN_MANIFEST);
        let meta = std::fs::symlink_metadata(&manifest_path)
            .map_err(|e| PluginError::Manifest(format!("{PLUGIN_MANIFEST}: {e}")))?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(PluginError::Manifest(
                "manifest must be a regular file".into(),
            ));
        }
        let manifest: PluginManifest = toml::from_str(&std::fs::read_to_string(manifest_path)?)
            .map_err(|e| PluginError::Manifest(e.to_string()))?;
        if manifest.name.trim().is_empty()
            || manifest.version.trim().is_empty()
            || manifest.command.is_empty()
            || manifest.command[0].trim().is_empty()
        {
            return Err(PluginError::Manifest(
                "name, version and command are required".into(),
            ));
        }
        if manifest.ummp_api != PLUGIN_API {
            return Err(PluginError::Api {
                found: manifest.ummp_api,
                supported: PLUGIN_API,
            });
        }
        let candidate = resolve_within(&directory, Path::new(&manifest.command[0]))
            .map_err(|_| PluginError::Manifest("command must stay inside plugin folder".into()))?;
        let program = candidate
            .canonicalize()
            .map_err(|e| PluginError::Manifest(format!("plugin executable unavailable: {e}")))?;
        if !program.starts_with(&directory) || !program.is_file() || forbidden(&program) {
            return Err(PluginError::Manifest(
                "command must resolve to an allowed file inside plugin folder".into(),
            ));
        }
        Ok(Self {
            info: PluginInfo {
                name: manifest.name,
                version: manifest.version,
                kind: manifest.kind,
                ummp_api: manifest.ummp_api,
                directory,
            },
            program,
            args: manifest.command.into_iter().skip(1).collect(),
        })
    }

    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    fn invoke(&self, request: &Value) -> Result<Value, PluginError> {
        self.invoke_with_env(request, scrubbed_env())
    }

    fn invoke_with_env(
        &self,
        request: &Value,
        env: Vec<(String, String)>,
    ) -> Result<Value, PluginError> {
        let mut input =
            serde_json::to_vec(request).map_err(|e| PluginError::Protocol(e.to_string()))?;
        input.push(b'\n');
        if input.len() > PLUGIN_STDOUT_LIMIT {
            return Err(PluginError::RequestLimit(PLUGIN_STDOUT_LIMIT));
        }
        let spec = ProcSpec {
            program: self.program.clone(),
            args: self.args.clone(),
            cwd: self.info.directory.clone(),
            env,
            timeout: PLUGIN_TIMEOUT,
        };
        let (status, output) = match run_with_input(&spec, &input, PLUGIN_STDOUT_LIMIT) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                return Err(PluginError::OutputLimit(PLUGIN_STDOUT_LIMIT))
            }
            Err(e) => return Err(PluginError::Io(e)),
        };
        match status {
            ProcStatus::TimedOut => return Err(PluginError::Timeout(PLUGIN_TIMEOUT.as_secs())),
            ProcStatus::Exited(0) => {}
            ProcStatus::Exited(code) => {
                return Err(PluginError::Protocol(format!(
                    "plugin exited with code {code}"
                )))
            }
            ProcStatus::WatchMatched => {
                return Err(PluginError::Protocol("unexpected process status".into()))
            }
        }
        decode_plugin_response(&output)
    }

    fn hello(&self) -> Result<(), PluginError> {
        let reply = self.invoke(&json!({"method":"hello"}))?;
        let api = reply
            .get("ummp_api")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| PluginError::Protocol("hello response has no ummp_api".into()))?;
        if api != PLUGIN_API {
            return Err(PluginError::Api {
                found: api,
                supported: PLUGIN_API,
            });
        }
        if reply.get("id").and_then(Value::as_str) != Some(&self.info.name) {
            return Err(PluginError::Protocol(
                "hello id does not match manifest name".into(),
            ));
        }
        Ok(())
    }

    fn files(cx: &AdapterContext<'_>) -> Result<Vec<PluginFile>, PluginError> {
        let mut files = Vec::new();
        for rel in cx.files {
            if forbidden(rel) || sensitive(rel) {
                continue;
            }
            let path = resolve_within(cx.root, rel)
                .map_err(|_| PluginError::Protocol("project file path escapes root".into()))?;
            let meta = std::fs::metadata(&path)?;
            if meta.len() > MAX_FILE_BYTES {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(path) {
                files.push(PluginFile {
                    path: rel.to_string_lossy().replace('\\', "/"),
                    content,
                });
            }
        }
        Ok(files)
    }

    fn validate_path(root: &Path, rel: &Path, files: &[PathBuf]) -> Result<PathBuf, PluginError> {
        if rel.as_os_str().is_empty()
            || rel.is_absolute()
            || forbidden(rel)
            || sensitive(rel)
            || !files.iter().any(|path| path == rel)
        {
            return Err(PluginError::Protocol(format!(
                "plugin returned a disallowed path: {}",
                rel.display()
            )));
        }
        let absolute = resolve_within(root, rel)
            .map_err(|_| PluginError::Protocol("plugin path escapes project root".into()))?;
        let canonical_root = root.canonicalize()?;
        let canonical = absolute.canonicalize()?;
        if !canonical.starts_with(canonical_root) || !canonical.is_file() {
            return Err(PluginError::Protocol(
                "plugin path resolves outside project root".into(),
            ));
        }
        Ok(canonical)
    }
}

fn forbidden(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("jar" | "class")
    )
}
fn sensitive(path: &Path) -> bool {
    path.components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|part| {
            let part = part.to_ascii_lowercase();
            let private_key = [".pem", ".key", ".p12", ".pfx", ".jks", ".ppk", ".keystore"]
                .iter()
                .any(|suffix| part.ends_with(suffix))
                || matches!(
                    part.as_str(),
                    "id_rsa" | "id_ed25519" | "id_ecdsa" | "id_dsa"
                );
            let secret_config = ["credential", "secret", "token", "auth"]
                .iter()
                .any(|word| part.contains(word))
                && [
                    ".json",
                    ".yaml",
                    ".yml",
                    ".toml",
                    ".properties",
                    ".txt",
                    ".conf",
                ]
                .iter()
                .any(|suffix| part.ends_with(suffix));
            matches!(
                part.as_str(),
                ".ssh"
                    | ".aws"
                    | ".azure"
                    | ".docker"
                    | ".kube"
                    | ".gnupg"
                    | "gcloud"
                    | ".npmrc"
                    | "settings.xml"
                    | "gradle.properties"
                    | "local.properties"
                    | ".netrc"
                    | ".pypirc"
                    | ".git-credentials"
                    | "credentials"
                    | "secrets"
                    | "token"
                    | "tokens"
            ) || part == ".env"
                || part.starts_with(".env.")
                || private_key
                || secret_config
        })
}

impl TechAdapter for PluginAdapter {
    fn id(&self) -> &str {
        &self.info.name
    }
    fn level(&self) -> Level {
        Level::Migrate
    }

    fn findings(&self, cx: &AdapterContext<'_>) -> Vec<TechFinding> {
        let result = (|| {
            let files = Self::files(cx)?;
            let value = self.invoke(&json!({"method":"findings","files":files}))?;
            let findings: Vec<TechFinding> =
                serde_json::from_value(value).map_err(|e| PluginError::Protocol(e.to_string()))?;
            for finding in &findings {
                Self::validate_path(cx.root, Path::new(&finding.file), cx.files)?;
            }
            Ok::<_, PluginError>(findings)
        })();
        result.unwrap_or_else(|e| {
            vec![TechFinding {
                tech: self.info.name.clone(),
                handling: Handling::Manual,
                file: String::new(),
                line: None,
                message: format!("plugin failed: {e}"),
            }]
        })
    }

    fn edits(&self, cx: &AdapterContext<'_>) -> Result<EditSet, MigrationError> {
        let Some(plan) = cx.plan else {
            return Ok(EditSet::default());
        };
        let files = Self::files(cx).map_err(|e| MigrationError::InvalidRequest(e.to_string()))?;
        let value = self
            .invoke(&json!({"method":"edits","files":files,"target":{
                "minecraft":plan.request.target_minecraft,"loader":plan.request.target_loader
            }}))
            .map_err(|e| MigrationError::InvalidRequest(e.to_string()))?;
        let edits: Vec<PluginEdit> = serde_json::from_value(value)
            .map_err(|e| MigrationError::InvalidRequest(format!("invalid plugin edits: {e}")))?;
        let mut set = EditSet::default();
        for edit in edits {
            let path = Self::validate_path(cx.root, &edit.path, cx.files)
                .map_err(|e| MigrationError::InvalidRequest(e.to_string()))?;
            if std::fs::read_to_string(&path)? != edit.original {
                return Err(MigrationError::InvalidRequest(format!(
                    "plugin edit for {} is stale",
                    edit.path.display()
                )));
            }
            set.edits.push(FileEdit {
                path: edit.path,
                original: edit.original,
                updated: edit.updated,
                replacements: Vec::new(),
            });
        }
        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::AdapterRegistry;
    use crate::build::process::scrubbed_env_from;
    use crate::checkpoint::CheckpointStore;
    use crate::descriptor::DescriptorTarget;
    use crate::migration::{MigrationEngine, MigrationPlan, MigrationRequest, RunOptions};
    use crate::project::Project;
    use crate::resources::PackFormats;
    use std::process::Command;
    use uuid::Uuid;

    #[test]
    fn refuses_jar_class_and_secret_paths() {
        assert!(forbidden(Path::new("a/B.JAR")));
        assert!(forbidden(Path::new("a/B.class")));
        assert!(sensitive(Path::new(".env.production")));
        assert!(sensitive(Path::new("config/credentials.json")));
        assert!(sensitive(Path::new(".ssh/id_ed25519")));
        assert!(sensitive(Path::new(".aws/config")));
        assert!(sensitive(Path::new(".docker/config.json")));
        assert!(sensitive(Path::new("certs/private.pem")));
        assert!(sensitive(Path::new("keys/release.p12")));
        assert!(!sensitive(Path::new("src/TokenManager.java")));
        assert!(!sensitive(Path::new("src/SecretManager.java")));
        assert!(!forbidden(Path::new("src/Main.java")));
    }

    #[test]
    fn rejects_parent_executable_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(PLUGIN_MANIFEST),
            "name='demo'\nversion='1'\nkind='rules'\ncommand=['../outside']\nummp_api=1\n",
        )
        .unwrap();
        assert!(PluginAdapter::load(dir.path()).is_err());
    }

    #[test]
    fn ummp_api_2_refused_with_message() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(PLUGIN_MANIFEST),
            "name='demo'\nversion='1'\nkind='rules'\ncommand=['bin/plugin']\nummp_api=2\n",
        )
        .unwrap();
        let error = PluginAdapter::load(dir.path()).unwrap_err();
        assert!(error.to_string().contains("API 2"));
        assert!(error.to_string().contains("API 1"));
    }

    fn fake_plugin(dir: &Path) {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/tests/fixtures/fake_plugin.rs");
        let binary_name = if cfg!(windows) {
            "fake-plugin.exe"
        } else {
            "fake-plugin"
        };
        let status = Command::new("rustc")
            .arg(fixture)
            .arg("-o")
            .arg(dir.join(binary_name))
            .status()
            .expect("rustc must be available for the fake plugin fixture");
        assert!(status.success(), "fake plugin fixture must compile");
        std::fs::write(
            dir.join(PLUGIN_MANIFEST),
            format!(
                "name='demo'\nversion='1.0.0'\nkind='rules'\ncommand=['{binary_name}']\nummp_api=1\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn plugin_edit_is_checkpointed_and_rollback_restores_source() {
        let plugin_dir = tempfile::tempdir().unwrap();
        fake_plugin(plugin_dir.path());
        let project_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(project_dir.path().join("src")).unwrap();
        std::fs::write(
            project_dir.path().join("src/Main.java"),
            "class DemoWidget {}",
        )
        .unwrap();
        let project = Project::new(project_dir.path().canonicalize().unwrap());
        let rel = PathBuf::from("src/Main.java");
        let files = vec![rel];
        let plan = MigrationPlan {
            request: MigrationRequest {
                source_minecraft: "1.20.1".into(),
                target_minecraft: "1.21.1".into(),
                target_loader: "fabric".into(),
                min_confidence: 0.8,
                source_loader: Some("fabric".into()),
            },
            stages: Vec::new(),
            skipped: Vec::new(),
            breaking_changes: Vec::new(),
            recipes: Vec::new(),
            descriptor_target: DescriptorTarget {
                minecraft: "1.21.1".into(),
                loader: "fabric".into(),
                loader_version: None,
            },
            java: None,
            pack_formats: PackFormats::default(),
            datapack_renames: false,
        };
        let plugin = PluginAdapter::load(plugin_dir.path()).unwrap();
        let cx = AdapterContext {
            root: &project.root,
            files: &files,
            plan: Some(&plan),
            source_loader: Some("fabric".into()),
        };
        let set = plugin.edits(&cx).unwrap();
        assert_eq!(set.edits.len(), 1);
        let engine =
            MigrationEngine::default().with_adapters(AdapterRegistry::new(vec![Box::new(plugin)]));
        let snapshots = tempfile::tempdir().unwrap();
        let mut store = CheckpointStore::new();
        let checkpoint = engine
            .apply_edit_set(
                &set,
                &project,
                &mut store,
                &RunOptions {
                    dry_run: false,
                    snapshot_root: snapshots.path(),
                    session_id: Uuid::new_v4(),
                },
            )
            .unwrap()
            .unwrap();
        let file = project.root.join("src/Main.java");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "class DemoGadget {}"
        );
        store.restore(checkpoint, &project.root).unwrap();
        assert_eq!(
            std::fs::read_to_string(file).unwrap(),
            "class DemoWidget {}"
        );
    }

    #[test]
    fn plugin_env_has_no_keys_or_values() {
        let plugin_dir = tempfile::tempdir().unwrap();
        fake_plugin(plugin_dir.path());
        let plugin = PluginAdapter::load(plugin_dir.path()).unwrap();
        let env = scrubbed_env_from(vec![
            ("PATH".to_owned(), "/usr/bin".to_owned()),
            (
                "OPENAI_API_KEY".to_owned(),
                "synthetic-openai-secret".to_owned(),
            ),
            (
                "GITHUB_TOKEN".to_owned(),
                "synthetic-github-secret".to_owned(),
            ),
            (
                "UMMP_REST_TOKEN".to_owned(),
                "synthetic-rest-secret".to_owned(),
            ),
        ]);
        let echoed = plugin
            .invoke_with_env(&json!({"method":"env"}), env)
            .unwrap();
        for key in ["OPENAI_API_KEY", "GITHUB_TOKEN", "UMMP_REST_TOKEN"] {
            assert!(echoed.get(key).is_none(), "plugin received {key}");
        }
        let echoed = serde_json::to_string(&echoed).unwrap();
        assert!(!echoed.contains("synthetic-openai-secret"));
        assert!(!echoed.contains("synthetic-github-secret"));
        assert!(!echoed.contains("synthetic-rest-secret"));
    }

    #[test]
    fn rejects_plugin_returned_path_traversal() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("inside.txt"), "safe").unwrap();
        let files = vec![PathBuf::from("inside.txt")];
        assert!(
            PluginAdapter::validate_path(root.path(), Path::new("../../outside"), &files).is_err()
        );
    }

    #[test]
    fn plugin_protocol_decoder_accepts_one_json_line() {
        assert_eq!(
            decode_plugin_response(b"{\"ok\":true}\n").unwrap(),
            json!({"ok": true})
        );
    }

    #[test]
    fn plugin_protocol_decoder_rejects_multiple_lines_and_invalid_utf8() {
        assert!(decode_plugin_response(b"{}\n{}\n").is_err());
        assert!(decode_plugin_response(&[0xff]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn hanging_plugin_killed_at_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let spec = ProcSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "sleep 5".into()],
            cwd: dir.path().into(),
            env: scrubbed_env(),
            timeout: Duration::from_millis(100),
        };
        let (status, _) = run_with_input(&spec, b"{}\n", 1024).unwrap();
        assert_eq!(status, ProcStatus::TimedOut);
    }

    #[cfg(unix)]
    #[test]
    fn kills_descendant_that_keeps_protocol_pipe_open() {
        let dir = tempfile::tempdir().unwrap();
        let spec = ProcSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "(sleep 5) & exit 0".into()],
            cwd: dir.path().into(),
            env: scrubbed_env(),
            timeout: Duration::from_millis(100),
        };
        let (status, _) = run_with_input(&spec, b"{}\n", 1024).unwrap();
        assert_eq!(status, ProcStatus::TimedOut);
    }

    #[cfg(unix)]
    #[test]
    fn plugin_stdout_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let spec = ProcSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "head -c 4096 /dev/zero".into()],
            cwd: dir.path().into(),
            env: scrubbed_env(),
            timeout: Duration::from_secs(2),
        };
        let error = run_with_input(&spec, b"{}\n", 128).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }
}
