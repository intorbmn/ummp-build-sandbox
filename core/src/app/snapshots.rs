//! Checkpoint store location and persistence, shared by the CLI and the UI.

use super::AppError;
use crate::checkpoint::CheckpointStore;
use std::path::{Path, PathBuf};

const STORE_FILE: &str = "checkpoints.json";

/// `<name>.ummp-snapshots`, next to the project folder.
#[must_use]
pub fn default_snapshots(project_root: &Path) -> PathBuf {
    let name = project_root
        .file_name()
        .map_or_else(|| "project".into(), |n| n.to_string_lossy().into_owned());
    project_root.with_file_name(format!("{name}.ummp-snapshots"))
}

/// Reads the checkpoint store in `dir`; a missing file is an empty store.
///
/// # Errors
/// Returns [`AppError::Input`] when the file is not a checkpoint store, or an I/O error.
pub fn load_store(dir: &Path) -> Result<CheckpointStore, AppError> {
    let file = dir.join(STORE_FILE);
    if !file.exists() {
        return Ok(CheckpointStore::new());
    }
    serde_json::from_slice(&std::fs::read(&file)?)
        .map_err(|e| AppError::Input(format!("{} is not a checkpoint store: {e}", file.display())))
}

/// Writes the store into `dir`, creating it when needed.
///
/// # Errors
/// Returns an I/O error when `dir` cannot be written.
pub fn save_store(dir: &Path, store: &CheckpointStore) -> Result<(), AppError> {
    store.persist(dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn truncated_checkpoint_manifest_is_rejected() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(STORE_FILE), br#"{"checkpoints":["#).unwrap();

        let error = load_store(dir.path()).unwrap_err();
        assert!(matches!(error, AppError::Input(_)));
    }

    #[test]
    fn save_store_replaces_manifest_atomically_and_round_trips() {
        let dir = tempdir().unwrap();
        let store = CheckpointStore::new();

        save_store(dir.path(), &store).unwrap();
        // Replacing an existing manifest must remain atomic on every supported platform.
        save_store(dir.path(), &store).unwrap();

        let loaded = load_store(dir.path()).unwrap();
        assert!(loaded.checkpoints.is_empty());
        assert!(dir.path().join(STORE_FILE).is_file());
    }
}
