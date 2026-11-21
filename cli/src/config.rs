//! CLI configuration and precedence: flags > environment > file > built-ins.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct CliConfig {
    pub output_root: Option<PathBuf>,
    pub kb: Option<PathBuf>,
    pub sandbox_repo: Option<String>,
    pub ai_provider: Option<String>,
    pub openai_base_url: Option<String>,
    pub allowed_roots: Option<Vec<PathBuf>>,
}

impl CliConfig {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Self::load_with_path(None)
    }

    pub fn load_with_path(explicit: Option<&Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let cwd = std::env::current_dir()?;
        let data_dir = default_data_dir();
        Self::load_from(explicit, &cwd, data_dir.as_deref(), |key| {
            std::env::var(key).ok()
        })
    }

    fn load_from(
        explicit: Option<&Path>,
        cwd: &Path,
        data_dir: Option<&Path>,
        mut env: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let path = explicit.map(Path::to_path_buf).or_else(|| {
            let local = cwd.join("ummp.toml");
            if local.is_file() {
                Some(local)
            } else {
                data_dir
                    .map(|dir| dir.join("ummp.toml"))
                    .filter(|path| path.is_file())
            }
        });

        let mut config = match path {
            Some(path) => load_file(&path)?,
            None => Self::default(),
        };

        config.output_root = env_value(&mut env, "UMMP_OUTPUT_ROOT")
            .or_else(|| env_value(&mut env, "UMMP_OUTPUT_DIR"))
            .map(PathBuf::from)
            .or(config.output_root);
        config.kb = env_value(&mut env, "UMMP_KB")
            .map(PathBuf::from)
            .or(config.kb);
        config.sandbox_repo = env_value(&mut env, "UMMP_BUILD_REPO").or(config.sandbox_repo);
        config.ai_provider = env_value(&mut env, "UMMP_AI_PROVIDER")
            .or_else(|| env_value(&mut env, "UMMP_PROVIDER"))
            .or(config.ai_provider);
        config.openai_base_url =
            env_value(&mut env, "UMMP_OPENAI_BASE_URL").or(config.openai_base_url);

        Ok(config)
    }

    #[must_use]
    pub fn output_dir(&self, cli: Option<PathBuf>) -> Option<PathBuf> {
        cli.or_else(|| self.output_root.clone())
    }

    #[must_use]
    pub fn kb_path(&self, cli: Option<PathBuf>) -> Option<PathBuf> {
        cli.or_else(|| self.kb.clone())
    }

    #[must_use]
    pub fn sandbox_repo(&self, cli: Option<String>) -> Option<String> {
        cli.or_else(|| self.sandbox_repo.clone())
    }

    #[must_use]
    pub fn provider(&self, cli: Option<String>) -> String {
        cli.or_else(|| self.ai_provider.clone())
            .unwrap_or_else(|| "anthropic".into())
    }

    #[must_use]
    pub fn openai_base_url(&self, cli: Option<String>) -> Option<String> {
        cli.or_else(|| self.openai_base_url.clone())
    }

    /// Roots the local REST API may access; defaults to the current user home folder.
    #[must_use]
    pub fn allowed_roots(&self) -> Vec<PathBuf> {
        self.allowed_roots.clone().unwrap_or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from)
                .into_iter()
                .collect()
        })
    }
}

fn load_file(path: &Path) -> Result<CliConfig, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    if text.lines().any(is_secret_field) {
        return Err(
            "secret-like fields are not allowed in ummp.toml; use `ummp ai-key set`".into(),
        );
    }
    ummp_core::config::load_any(path)
        .map_err(|_| "invalid ummp.toml: check its syntax and supported field names".into())
}

fn is_secret_field(line: &str) -> bool {
    let key = line
        .split('#')
        .next()
        .unwrap_or_default()
        .split_once('=')
        .map_or("", |(key, _)| {
            key.trim().trim_matches('"').trim_matches('\'')
        })
        .to_ascii_lowercase();
    ["key", "token", "secret", "password"]
        .iter()
        .any(|part| key.contains(part))
}

fn env_value(env: &mut impl FnMut(&str) -> Option<String>, key: &str) -> Option<String> {
    env(key).filter(|value| !value.trim().is_empty())
}

#[must_use]
pub fn data_dir() -> PathBuf {
    default_data_dir().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".ummp")
    })
}

fn default_data_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("UMMP_DATA_DIR").filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path));
    }

    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("io.github.intorbmn.ummp"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join("Library/Application Support/io.github.intorbmn.ummp"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|p| p.join(".local/share"))
            })
            .map(|path| path.join("io.github.intorbmn.ummp"))
    }
    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::CliConfig;
    use std::path::{Path, PathBuf};

    fn load(
        explicit: Option<&Path>,
        cwd: &Path,
        data_dir: Option<&Path>,
        values: &[(&str, &str)],
    ) -> Result<CliConfig, Box<dyn std::error::Error>> {
        CliConfig::load_from(explicit, cwd, data_dir, |key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    #[test]
    fn cli_values_override_config_values() {
        let config = CliConfig {
            output_root: Some(PathBuf::from("from-file")),
            kb: Some(PathBuf::from("file-mappings.db")),
            sandbox_repo: Some("file/repo".into()),
            ai_provider: Some("openai".into()),
            ..CliConfig::default()
        };
        assert_eq!(
            config.output_dir(Some("from-cli".into())),
            Some("from-cli".into())
        );
        assert_eq!(
            config.kb_path(Some("cli-mappings.db".into())),
            Some(PathBuf::from("cli-mappings.db"))
        );
        assert_eq!(
            config.kb_path(None),
            Some(PathBuf::from("file-mappings.db"))
        );
        assert_eq!(
            config.sandbox_repo(Some("cli/repo".into())).as_deref(),
            Some("cli/repo")
        );
        assert_eq!(config.provider(Some("anthropic".into())), "anthropic");
    }

    #[test]
    fn config_file_fields_are_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ummp.toml");
        std::fs::write(
            &path,
            r#"output_root = "out"
kb = "mappings.db"
sandbox_repo = "owner/repo"
ai_provider = "openai"
openai_base_url = "http://localhost:11434/v1"
allowed_roots = ["D:/Minecraft/mods"]
"#,
        )
        .unwrap();
        let config = load(Some(&path), dir.path(), None, &[]).unwrap();
        assert_eq!(config.output_root, Some(PathBuf::from("out")));
        assert_eq!(config.kb, Some(PathBuf::from("mappings.db")));
        assert_eq!(config.ai_provider.as_deref(), Some("openai"));
        assert_eq!(
            config.openai_base_url.as_deref(),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(
            config.allowed_roots,
            Some(vec![PathBuf::from("D:/Minecraft/mods")])
        );
    }

    #[test]
    fn local_config_precedes_shared_data_config() {
        let cwd = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(cwd.path().join("ummp.toml"), "ai_provider = \"openai\"").unwrap();
        std::fs::write(data.path().join("ummp.toml"), "ai_provider = \"anthropic\"").unwrap();
        let config = load(None, cwd.path(), Some(data.path()), &[]).unwrap();
        assert_eq!(config.ai_provider.as_deref(), Some("openai"));
    }

    #[test]
    fn explicit_config_precedes_local_config() {
        let cwd = tempfile::tempdir().unwrap();
        let explicit = tempfile::tempdir().unwrap();
        std::fs::write(cwd.path().join("ummp.toml"), "ai_provider = \"anthropic\"").unwrap();
        let path = explicit.path().join("custom.toml");
        std::fs::write(&path, "ai_provider = \"openai\"").unwrap();
        let config = load(Some(&path), cwd.path(), None, &[]).unwrap();
        assert_eq!(config.ai_provider.as_deref(), Some("openai"));
    }

    #[test]
    fn environment_overrides_file_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ummp.toml");
        std::fs::write(
            &path,
            "output_root = \"from-file\"\nai_provider = \"anthropic\"",
        )
        .unwrap();
        let config = load(
            Some(&path),
            dir.path(),
            None,
            &[
                ("UMMP_OUTPUT_ROOT", "from-env"),
                ("UMMP_AI_PROVIDER", "openai"),
            ],
        )
        .unwrap();
        assert_eq!(config.output_root, Some(PathBuf::from("from-env")));
        assert_eq!(config.ai_provider.as_deref(), Some("openai"));
    }

    #[test]
    fn key_like_fields_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ummp.toml");
        std::fs::write(&path, "anthropic_api_key = \"synthetic-secret\"").unwrap();
        let error = load(Some(&path), dir.path(), None, &[])
            .unwrap_err()
            .to_string();
        assert!(error.contains("secret-like fields"));
        assert!(!error.contains("synthetic-secret"));
    }

    #[test]
    fn blank_environment_values_do_not_override_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ummp.toml");
        std::fs::write(&path, "ai_provider = \"openai\"").unwrap();
        let config = load(Some(&path), dir.path(), None, &[("UMMP_AI_PROVIDER", "  ")]).unwrap();
        assert_eq!(config.ai_provider.as_deref(), Some("openai"));
    }

    #[test]
    fn built_in_provider_is_anthropic() {
        assert_eq!(CliConfig::default().provider(None), "anthropic");
    }
}
