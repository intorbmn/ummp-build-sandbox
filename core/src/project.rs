//! Application: project manager.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

use crate::filesystem::{FileSystem, FsError, LocalFileSystem};
use crate::metadata::ProjectMetadata;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("io: {0}")]
    Fs(#[from] FsError),
    #[error("invalid project: {0}")]
    Invalid(String),
    #[error("metadata parse: {0}")]
    Metadata(#[from] serde_json::Error),
}

pub trait ProjectManager: Send + Sync {
    fn open(&self, path: &Path) -> Result<Project, ProjectError>;
    fn metadata(&self, project: &Project) -> Result<ProjectMetadata, ProjectError>;
}

pub struct Project {
    pub root: PathBuf,
    pub fs: Arc<dyn FileSystem>,
}

impl std::fmt::Debug for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Project")
            .field("root", &self.root)
            .field("fs", &"<FileSystem>")
            .finish()
    }
}

impl Project {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            fs: Arc::new(LocalFileSystem),
        }
    }
}

pub struct DefaultProjectManager;

impl ProjectManager for DefaultProjectManager {
    fn open(&self, path: &Path) -> Result<Project, ProjectError> {
        if !path.is_dir() {
            return Err(ProjectError::Invalid("path is not a directory".into()));
        }
        let root = crate::filesystem::validate_root(path).map_err(ProjectError::Fs)?;
        Ok(Project::new(root))
    }

    fn metadata(&self, project: &Project) -> Result<ProjectMetadata, ProjectError> {
        let meta_path = project.root.join("ummp.json");
        if project.fs.exists(&meta_path) {
            let bytes = project.fs.read(&meta_path)?;
            Ok(serde_json::from_slice(&bytes)?)
        } else {
            Ok(ProjectMetadata::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_open_valid_directory() {
        let dir = tempdir().unwrap();
        let pm = DefaultProjectManager;
        let project = pm.open(dir.path()).unwrap();
        assert_eq!(project.root, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn test_open_invalid_path() {
        let pm = DefaultProjectManager;
        let err = pm.open(Path::new("/nonexistent/path")).unwrap_err();
        assert!(matches!(err, ProjectError::Invalid(_)));
    }

    #[test]
    fn test_metadata_no_file_returns_default() {
        let dir = tempdir().unwrap();
        let pm = DefaultProjectManager;
        let project = pm.open(dir.path()).unwrap();
        let meta = pm.metadata(&project).unwrap();
        assert_eq!(meta.mods.len(), 0);
        assert_eq!(meta.languages.len(), 0);
    }

    #[test]
    fn test_metadata_reads_ummp_json() {
        let dir = tempdir().unwrap();
        let json = r#"{"mods":[{"mod_id":"test","version":"1.0.0","name":"Test Mod","loader":"Fabric","minecraft":{"major":1,"minor":20,"patch":1},"dependencies":[]}],"languages":["java"]}"#;
        fs::write(dir.path().join("ummp.json"), json).unwrap();
        let pm = DefaultProjectManager;
        let project = pm.open(dir.path()).unwrap();
        let meta = pm.metadata(&project).unwrap();
        assert_eq!(meta.mods.len(), 1);
        assert_eq!(meta.mods[0].mod_id, "test");
        assert_eq!(meta.languages, vec!["java"]);
    }
}
