//! Configuration loader supporting YAML and JSON.

use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(String),
}

pub fn load_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ConfigError> {
    let bytes = std::fs::read(path)?;
    serde_yaml::from_slice(&bytes).map_err(|e| ConfigError::Parse(e.to_string()))
}

pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ConfigError> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|e| ConfigError::Parse(e.to_string()))
}

pub fn load_any<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ConfigError> {
    match path.extension().and_then(|s| s.to_str()) {
        Some("yaml" | "yml") => load_yaml(path),
        Some("json") => load_json(path),
        _ => Err(ConfigError::Parse("unknown config format".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::fs;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestConfig {
        value: u32,
    }

    #[test]
    fn test_load_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, "value: 42").unwrap();
        let cfg: TestConfig = load_yaml(&path).unwrap();
        assert_eq!(cfg.value, 42);
    }

    #[test]
    fn test_load_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{"value": 42}"#).unwrap();
        let cfg: TestConfig = load_json(&path).unwrap();
        assert_eq!(cfg.value, 42);
    }

    #[test]
    fn test_load_any() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, "value: 1").unwrap();
        let cfg: TestConfig = load_any(&path).unwrap();
        assert_eq!(cfg.value, 1);
    }
}
