//! Shared, interim storage for AI provider credentials.
//!
//! Callers depend on `AiKeyStore`; the JSON file is an implementation detail and can
//! be replaced without changing the CLI or desktop adapters.

use super::{AppError, Secret};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const CREDENTIALS_FILE: &str = "credentials.json";
const SCHEMA: u32 = 1;
const MAX_CREDENTIALS_BYTES: u64 = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderKind {
    Anthropic,
    Openai,
}

impl AiProviderKind {
    pub const fn env_name(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::Openai => "OPENAI_API_KEY",
        }
    }
}

pub trait AiKeyStore: Send + Sync {
    fn get(&self, provider: AiProviderKind) -> Result<Option<Secret>, AppError>;
    fn set(&self, provider: AiProviderKind, key: &Secret) -> Result<Option<String>, AppError>;
    fn delete(&self, provider: AiProviderKind) -> Result<(), AppError>;
    fn masked(&self, provider: AiProviderKind) -> Result<Option<String>, AppError>;
    fn openai_base_url(&self) -> Result<Option<String>, AppError>;
    fn set_openai_base_url(&self, value: Option<&str>) -> Result<(), AppError>;
}

pub struct FileKeyStore {
    credentials: CredentialsFile,
}

impl FileKeyStore {
    #[must_use]
    pub fn in_dir(data_dir: &Path) -> Self {
        Self {
            credentials: CredentialsFile::in_dir(data_dir),
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.credentials.path()
    }

    pub fn openai_base_url(&self) -> Result<Option<String>, AppError> {
        self.credentials.openai_base_url()
    }

    pub fn set_openai_base_url(&self, value: Option<&str>) -> Result<(), AppError> {
        self.credentials.set_openai_base_url(value)
    }
}

impl AiKeyStore for FileKeyStore {
    fn get(&self, provider: AiProviderKind) -> Result<Option<Secret>, AppError> {
        self.credentials.get(provider)
    }

    fn set(&self, provider: AiProviderKind, key: &Secret) -> Result<Option<String>, AppError> {
        self.credentials.set(provider, key)
    }

    fn delete(&self, provider: AiProviderKind) -> Result<(), AppError> {
        self.credentials.delete(provider)
    }

    fn masked(&self, provider: AiProviderKind) -> Result<Option<String>, AppError> {
        self.credentials.masked(provider)
    }

    fn openai_base_url(&self) -> Result<Option<String>, AppError> {
        self.credentials.openai_base_url()
    }

    fn set_openai_base_url(&self, value: Option<&str>) -> Result<(), AppError> {
        self.credentials.set_openai_base_url(value)
    }
}

pub struct CredentialsFile {
    path: PathBuf,
}

impl CredentialsFile {
    #[must_use]
    pub fn in_dir(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(CREDENTIALS_FILE),
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self, provider: AiProviderKind) -> Result<Option<Secret>, AppError> {
        let file = self.read()?;
        Ok(Self::key(&file, provider).map(Secret::new))
    }

    /// Saves one provider key and returns a non-fatal provider-specific warning, if any.
    pub fn set(&self, provider: AiProviderKind, key: &Secret) -> Result<Option<String>, AppError> {
        let value = validate_key(key.expose())?;
        let warning = (provider == AiProviderKind::Anthropic && !value.starts_with("sk-ant-"))
            .then_some(
                "This key does not use the usual sk-ant- prefix; it will still be saved."
                    .to_owned(),
            );
        self.with_exclusive_lock(|| {
            let mut file = self.read()?;
            *Self::key_mut(&mut file, provider) = Some(value);
            self.write(&file)?;
            Ok(warning)
        })
    }

    pub fn delete(&self, provider: AiProviderKind) -> Result<(), AppError> {
        self.with_exclusive_lock(|| {
            let mut file = self.read()?;
            *Self::key_mut(&mut file, provider) = None;
            self.write(&file)
        })
    }

    pub fn masked(&self, provider: AiProviderKind) -> Result<Option<String>, AppError> {
        Ok(self.get(provider)?.map(|key| mask(key.expose())))
    }

    pub fn openai_base_url(&self) -> Result<Option<String>, AppError> {
        Ok(self.read()?.openai_base_url)
    }

    pub fn set_openai_base_url(&self, value: Option<&str>) -> Result<(), AppError> {
        let value = validate_openai_base_url(value)?;
        self.with_exclusive_lock(|| {
            let mut file = self.read()?;
            file.openai_base_url = value;
            self.write(&file)
        })
    }

    /// Serializes credential read-modify-write operations across CLI and desktop processes.
    fn with_exclusive_lock<T>(
        &self,
        operation: impl FnOnce() -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| AppError::Input("credentials file has no parent directory".into()))?;
        std::fs::create_dir_all(parent)?;
        let lock_path = self.path.with_file_name("credentials.json.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock.lock()?;
        operation()
    }

    fn read(&self) -> Result<StoredCredentials, AppError> {
        if !self.path.exists() {
            return Ok(StoredCredentials::default());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&self.path)?
            .take(MAX_CREDENTIALS_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_CREDENTIALS_BYTES {
            return Err(AppError::Input(
                "credentials.json exceeds the supported size limit".into(),
            ));
        }
        let file: StoredCredentials = serde_json::from_slice(&bytes)
            .map_err(|_| AppError::Input("credentials.json is invalid or unsupported".into()))?;
        if file.schema != SCHEMA {
            return Err(AppError::Input(
                "credentials.json uses an unsupported schema version".into(),
            ));
        }
        Ok(file)
    }

    fn write(&self, file: &StoredCredentials) -> Result<(), AppError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| AppError::Input("credentials file has no parent directory".into()))?;
        std::fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec_pretty(file)
            .map_err(|_| AppError::Input("could not encode credentials".into()))?;

        let mut builder = tempfile::Builder::new();
        builder.prefix(".credentials-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o600));
        }
        let mut temporary = builder.tempfile_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&self.path)
            .map_err(|error| AppError::Io(error.error))?;
        Ok(())
    }

    fn key(file: &StoredCredentials, provider: AiProviderKind) -> Option<&str> {
        match provider {
            AiProviderKind::Anthropic => file.anthropic_api_key.as_deref(),
            AiProviderKind::Openai => file.openai_api_key.as_deref(),
        }
    }

    const fn key_mut(
        file: &mut StoredCredentials,
        provider: AiProviderKind,
    ) -> &mut Option<String> {
        match provider {
            AiProviderKind::Anthropic => &mut file.anthropic_api_key,
            AiProviderKind::Openai => &mut file.openai_api_key,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCredentials {
    schema: u32,
    #[serde(default)]
    anthropic_api_key: Option<String>,
    #[serde(default)]
    openai_api_key: Option<String>,
    #[serde(default)]
    openai_base_url: Option<String>,
}

impl Default for StoredCredentials {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            anthropic_api_key: None,
            openai_api_key: None,
            openai_base_url: None,
        }
    }
}

pub fn resolve_ai_key(
    data_dir: &Path,
    provider: AiProviderKind,
) -> Result<Option<Secret>, AppError> {
    let store = FileKeyStore::in_dir(data_dir);
    resolve_ai_key_from_store(&store, provider, std::env::var(provider.env_name()).ok())
}

/// Resolves environment first, then the injected store. Useful for desktop and test adapters.
pub fn resolve_ai_key_from_store(
    store: &dyn AiKeyStore,
    provider: AiProviderKind,
    env_value: Option<String>,
) -> Result<Option<Secret>, AppError> {
    if let Some(value) = env_value.filter(|value| !value.trim().is_empty()) {
        return Ok(Some(Secret::new(validate_key(&value)?)));
    }
    store.get(provider)
}

pub fn default_data_dir() -> Result<PathBuf, AppError> {
    let override_dir = std::env::var_os("UMMP_DATA_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    #[cfg(windows)]
    let fallback = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("io.github.intorbmn.ummp"));

    #[cfg(target_os = "macos")]
    let fallback = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|path| path.join("Library/Application Support/io.github.intorbmn.ummp"));

    #[cfg(all(unix, not(target_os = "macos")))]
    let fallback = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/share"))
        })
        .map(|path| path.join("io.github.intorbmn.ummp"));

    choose_data_dir(override_dir, fallback)
}

fn choose_data_dir(
    override_dir: Option<PathBuf>,
    fallback: Option<PathBuf>,
) -> Result<PathBuf, AppError> {
    override_dir
        .or(fallback)
        .ok_or_else(|| AppError::Input("could not determine the UMMP data directory".into()))
}

/// Validates and normalizes an optional OpenAI-compatible endpoint without writing it.
pub fn validate_openai_base_url(value: Option<&str>) -> Result<Option<String>, AppError> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    if value.is_some_and(|url| {
        !(url.starts_with("http://") || url.starts_with("https://"))
            || url.chars().any(char::is_control)
    }) {
        return Err(AppError::Input(
            "OpenAI base URL must use http:// or https:// and contain no control characters".into(),
        ));
    }
    Ok(value.map(str::to_owned))
}

fn validate_key(key: &str) -> Result<String, AppError> {
    let value = key.trim();
    if value.is_empty() || value.chars().count() > 512 || value.chars().any(char::is_control) {
        return Err(AppError::Input(
            "API key must be 1-512 characters and contain no control characters".into(),
        ));
    }
    Ok(value.to_owned())
}

fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() < 16 {
        return format!(
            "…{}",
            chars
                .iter()
                .rev()
                .take(2)
                .rev()
                .copied()
                .collect::<String>()
        );
    }
    format!(
        "{}…{}",
        chars.iter().take(7).copied().collect::<String>(),
        chars
            .iter()
            .rev()
            .take(4)
            .rev()
            .copied()
            .collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const ANTHROPIC: &str = "sk-ant-test-0000000000000000";
    const OPENAI: &str = "sk-openai-test-0000000000000000";

    /// ★ Mutation: remove the second provider write or delete the wrong key; preservation must fail.
    #[test]
    fn credentials_round_trip_and_keep_other_provider() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileKeyStore::in_dir(dir.path());
        store
            .set(AiProviderKind::Anthropic, &Secret::new(ANTHROPIC))
            .unwrap();
        store
            .set(AiProviderKind::Openai, &Secret::new(OPENAI))
            .unwrap();
        assert_eq!(
            store
                .get(AiProviderKind::Anthropic)
                .unwrap()
                .unwrap()
                .expose(),
            ANTHROPIC
        );
        assert_eq!(
            store.get(AiProviderKind::Openai).unwrap().unwrap().expose(),
            OPENAI
        );
        store.delete(AiProviderKind::Anthropic).unwrap();
        assert!(store.get(AiProviderKind::Anthropic).unwrap().is_none());
        assert_eq!(
            store.get(AiProviderKind::Openai).unwrap().unwrap().expose(),
            OPENAI
        );
    }

    #[test]
    fn concurrent_provider_updates_preserve_both_credentials_and_base_url() {
        use std::sync::{Arc, Barrier};

        let dir = tempfile::tempdir().unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();

        for (provider, key) in [
            (AiProviderKind::Anthropic, ANTHROPIC),
            (AiProviderKind::Openai, OPENAI),
        ] {
            let path = dir.path().to_path_buf();
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                let store = FileKeyStore::in_dir(&path);
                barrier.wait();
                for _ in 0..20 {
                    store.set(provider, &Secret::new(key)).unwrap();
                }
            }));
        }

        let path = dir.path().to_path_buf();
        let barrier_for_url = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            let store = FileKeyStore::in_dir(&path);
            barrier_for_url.wait();
            for _ in 0..20 {
                store
                    .set_openai_base_url(Some("https://api.example.test/v1"))
                    .unwrap();
            }
        }));

        for worker in workers {
            worker.join().unwrap();
        }

        let store = FileKeyStore::in_dir(dir.path());
        assert_eq!(
            store
                .get(AiProviderKind::Anthropic)
                .unwrap()
                .unwrap()
                .expose(),
            ANTHROPIC
        );
        assert_eq!(
            store.get(AiProviderKind::Openai).unwrap().unwrap().expose(),
            OPENAI
        );
        assert_eq!(
            store.openai_base_url().unwrap().as_deref(),
            Some("https://api.example.test/v1")
        );
    }

    /// ★ Mutation: prefer the file over the environment value; this assertion must fail.
    #[test]
    fn environment_value_overrides_file_without_global_environment_mutation() {
        let dir = tempfile::tempdir().unwrap();
        FileKeyStore::in_dir(dir.path())
            .set(AiProviderKind::Anthropic, &Secret::new(ANTHROPIC))
            .unwrap();
        let resolved = resolve_ai_key_from_store(
            &FileKeyStore::in_dir(dir.path()),
            AiProviderKind::Anthropic,
            Some("env-key-value".into()),
        )
        .unwrap();
        assert_eq!(resolved.unwrap().expose(), "env-key-value");
    }

    #[test]
    fn mask_shows_prefix_and_last_four_only() {
        assert_eq!(mask("sk-ant-test-0000000000000000"), "sk-ant-…0000");
        assert_eq!(mask("short-key"), "…ey");
    }

    #[test]
    fn set_rejects_empty_control_characters_and_long_keys() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileKeyStore::in_dir(dir.path());
        for key in ["  ", "bad\nkey", &"x".repeat(513)] {
            assert!(store
                .set(AiProviderKind::Openai, &Secret::new(key))
                .is_err());
        }
    }

    #[test]
    fn anthropic_prefix_warning_does_not_block_saving() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileKeyStore::in_dir(dir.path());
        let warning = store
            .set(AiProviderKind::Anthropic, &Secret::new("proxy-key-123456"))
            .unwrap();
        assert!(warning.is_some());
        assert!(store.get(AiProviderKind::Anthropic).unwrap().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn credentials_file_has_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let store = FileKeyStore::in_dir(dir.path());
        store
            .set(AiProviderKind::Anthropic, &Secret::new(ANTHROPIC))
            .unwrap();
        assert_eq!(
            fs::metadata(store.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn oversized_credentials_are_rejected_before_deserialization() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileKeyStore::in_dir(dir.path());
        fs::write(store.path(), vec![b'x'; 16 * 1024 + 1]).unwrap();
        let error = store
            .get(AiProviderKind::Anthropic)
            .unwrap_err()
            .to_string();
        assert!(error.contains("exceeds the supported size limit"));
    }

    #[test]
    fn corrupt_credentials_return_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileKeyStore::in_dir(dir.path());
        fs::write(store.path(), "{broken").unwrap();
        let error = store
            .get(AiProviderKind::Anthropic)
            .unwrap_err()
            .to_string();
        assert!(error.contains("credentials.json is invalid"));
    }

    #[test]
    fn delete_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileKeyStore::in_dir(dir.path());
        store.delete(AiProviderKind::Anthropic).unwrap();
        store.delete(AiProviderKind::Anthropic).unwrap();
        assert!(store.get(AiProviderKind::Anthropic).unwrap().is_none());
    }

    #[test]
    fn data_directory_override_is_respected() {
        let dir = tempfile::tempdir().unwrap();
        let fallback = dir.path().join("fallback");
        let resolved = choose_data_dir(Some(dir.path().to_path_buf()), Some(fallback)).unwrap();
        assert_eq!(resolved, dir.path());
    }

    #[test]
    fn unsupported_schema_returns_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileKeyStore::in_dir(dir.path());
        fs::write(store.path(), r#"{"schema":99}"#).unwrap();
        let error = store
            .get(AiProviderKind::Anthropic)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsupported schema version"));
        assert!(!error.contains("99"));
    }

    #[cfg(unix)]
    #[test]
    fn failed_write_leaves_previous_credentials_intact() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let dir = tempfile::tempdir().unwrap();
        let store = FileKeyStore::in_dir(dir.path());
        store
            .set(AiProviderKind::Anthropic, &Secret::new(ANTHROPIC))
            .unwrap();

        // GitHub's Linux runner is unprivileged; skip when invoked as root, which bypasses
        // directory write permissions and would make this failure injection meaningless.
        if fs::metadata(dir.path()).unwrap().uid() == 0 {
            return;
        }
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let result = store.set(
            AiProviderKind::Anthropic,
            &Secret::new("sk-ant-new-0000000000000000"),
        );
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();

        assert!(result.is_err());
        assert_eq!(
            store
                .get(AiProviderKind::Anthropic)
                .unwrap()
                .unwrap()
                .expose(),
            ANTHROPIC
        );
    }

    #[test]
    fn openai_base_url_is_persisted_and_validated() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileKeyStore::in_dir(dir.path());
        store
            .set_openai_base_url(Some(" https://api.example.test/v1 "))
            .unwrap();
        assert_eq!(
            store.openai_base_url().unwrap().as_deref(),
            Some("https://api.example.test/v1")
        );
        assert!(store.set_openai_base_url(Some("file:///tmp/key")).is_err());
    }
}
