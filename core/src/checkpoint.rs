//! Infrastructure: checkpoint / rollback for migration sessions.
//!
//! A checkpoint is a full copy of the project tree (minus build output and VCS
//! metadata) stored outside the project. `restore` makes the project tree
//! byte-identical to the snapshot again: changed files are rewritten, files
//! created after the snapshot are removed. See ADR 0006.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

/// Directories never copied into a snapshot and never touched by `restore`.
pub const SKIP_DIRS: &[&str] = &[".git", ".gradle", "build", "target", "out", "run"];

#[derive(Debug, Error)]
pub enum CheckpointError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("checkpoint not found: {0}")]
    NotFound(Uuid),
    #[error("invalid checkpoint location: {0}")]
    InvalidLocation(String),
    #[error("checkpoint metadata: {0}")]
    Metadata(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: Uuid,
    pub session_id: Uuid,
    pub step: String,
    pub snapshot_path: PathBuf,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CheckpointStore {
    pub checkpoints: Vec<Checkpoint>,
}

impl CheckpointStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, checkpoint: Checkpoint) {
        self.checkpoints.push(checkpoint);
    }

    pub fn latest(&self) -> Option<&Checkpoint> {
        self.checkpoints.last()
    }

    /// Persist checkpoint metadata atomically so an interrupted migration can be recovered.
    pub fn persist(&self, dir: &Path) -> Result<(), CheckpointError> {
        fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| CheckpointError::Metadata(error.to_string()))?;
        let destination = dir.join("checkpoints.json");
        let mut temporary = tempfile::NamedTempFile::new_in(dir)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&destination)
            .map_err(|error| CheckpointError::Io(error.error))?;
        #[cfg(unix)]
        fs::File::open(dir)?.sync_all()?;
        Ok(())
    }

    pub fn get(&self, id: Uuid) -> Option<&Checkpoint> {
        self.checkpoints.iter().find(|c| c.id == id)
    }

    /// Copy `project_root` into `snapshot_root/<checkpoint id>/` and record it.
    ///
    /// `snapshot_root` must not lie inside `project_root`, otherwise the
    /// snapshot would copy itself and a later restore would delete it.
    pub fn snapshot(
        &mut self,
        session_id: Uuid,
        step: &str,
        project_root: &Path,
        snapshot_root: &Path,
    ) -> Result<Checkpoint, CheckpointError> {
        let project = project_root.canonicalize()?;
        fs::create_dir_all(snapshot_root)?;
        let snap_root = snapshot_root.canonicalize()?;
        if snap_root.starts_with(&project) {
            return Err(CheckpointError::InvalidLocation(format!(
                "snapshot root {} is inside project {}",
                snap_root.display(),
                project.display()
            )));
        }

        let id = Uuid::new_v4();
        let dest = snap_root.join(id.to_string());
        fs::create_dir_all(&dest)?;
        for rel in collect_files(&project)? {
            let target = dest.join(&rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(project.join(&rel), &target)?;
            fs::File::open(&target)?.sync_all()?;
        }
        #[cfg(unix)]
        sync_directories(&dest)?;

        let cp = Checkpoint {
            id,
            session_id,
            step: step.to_string(),
            snapshot_path: dest,
            created_at: chrono::Utc::now(),
        };
        self.add(cp.clone());
        // Keep the completed snapshot if manifest publication reports an error:
        // the rename may already have happened and the durable manifest may reference it.
        // An orphaned snapshot is safe; a manifest pointing at a deleted snapshot is not.
        self.persist(&snap_root)?;
        Ok(cp)
    }

    /// Make `project_root` identical to checkpoint `id` (outside `SKIP_DIRS`).
    pub fn restore(&self, id: Uuid, project_root: &Path) -> Result<(), CheckpointError> {
        let cp = self.get(id).ok_or(CheckpointError::NotFound(id))?;
        restore_from(&cp.snapshot_path, project_root)
    }
}

#[cfg(unix)]
fn sync_directories(root: &Path) -> Result<(), std::io::Error> {
    let mut directories = vec![root.to_path_buf()];
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let child = entry.path();
                directories.push(child.clone());
                stack.push(child);
            }
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        fs::File::open(directory)?.sync_all()?;
    }
    Ok(())
}

/// Restore a project tree from a snapshot directory.
pub fn restore_from(snapshot: &Path, project_root: &Path) -> Result<(), CheckpointError> {
    if !snapshot.is_dir() {
        return Err(CheckpointError::InvalidLocation(format!(
            "snapshot {} does not exist",
            snapshot.display()
        )));
    }
    let project = project_root.canonicalize()?;
    let wanted: HashSet<PathBuf> = collect_files(snapshot)?.into_iter().collect();

    // 1. Remove files created after the snapshot.
    for rel in collect_files(&project)? {
        if !wanted.contains(&rel) {
            fs::remove_file(project.join(&rel))?;
        }
    }
    // 2. Remove directories that are now empty and did not exist in the snapshot.
    remove_new_empty_dirs(&project, &project, snapshot)?;
    // 3. Rewrite every snapshot file (covers modified and deleted files).
    for rel in &wanted {
        let target = project.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(snapshot.join(rel), &target)?;
        fs::File::open(&target)?.sync_all()?;
    }
    #[cfg(unix)]
    sync_directories(&project)?;
    Ok(())
}

fn is_skipped(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|n| SKIP_DIRS.contains(&n))
}

/// All regular files under `root`, as paths relative to `root`.
/// Symlinks are ignored (never followed) so a snapshot cannot escape the tree.
pub fn collect_files(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel_dir) = stack.pop() {
        for entry in fs::read_dir(root.join(&rel_dir))? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let rel = rel_dir.join(entry.file_name());
            if ft.is_dir() {
                if !is_skipped(&entry.file_name()) {
                    stack.push(rel);
                }
            } else if ft.is_file() {
                out.push(rel);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn remove_new_empty_dirs(root: &Path, dir: &Path, snapshot: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || is_skipped(&entry.file_name()) {
            continue;
        }
        let path = entry.path();
        remove_new_empty_dirs(root, &path, snapshot)?;
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let empty = fs::read_dir(&path)?.next().is_none();
        if empty && !snapshot.join(rel).is_dir() {
            fs::remove_dir(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_checkpoint_store() {
        let session_id = Uuid::new_v4();
        let mut store = CheckpointStore::new();

        let cp1 = Checkpoint {
            id: Uuid::new_v4(),
            session_id,
            step: "import".into(),
            snapshot_path: PathBuf::from("snap1"),
            created_at: chrono::Utc::now(),
        };
        let cp2 = Checkpoint {
            id: Uuid::new_v4(),
            session_id,
            step: "transform".into(),
            snapshot_path: PathBuf::from("snap2"),
            created_at: chrono::Utc::now(),
        };

        store.add(cp1);
        assert_eq!(store.latest().unwrap().step, "import");
        store.add(cp2);
        assert_eq!(store.latest().unwrap().step, "transform");
    }

    fn write(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[test]
    fn test_snapshot_and_restore_is_exact() {
        let proj = tempdir().unwrap();
        let snaps = tempdir().unwrap();
        let p = proj.path();
        write(&p.join("src/A.java"), "class A {}");
        write(&p.join("src/B.java"), "class B {}");
        write(&p.join("build/out.class"), "binary");

        let mut store = CheckpointStore::new();
        let cp = store
            .snapshot(Uuid::new_v4(), "pre-transform", p, snaps.path())
            .unwrap();
        // build output is not snapshotted
        assert!(!cp.snapshot_path.join("build").exists());

        // mutate: modify, delete, add file in a new directory
        write(&p.join("src/A.java"), "class A { int x; }");
        fs::remove_file(p.join("src/B.java")).unwrap();
        write(&p.join("src/new/C.java"), "class C {}");

        store.restore(cp.id, p).unwrap();

        assert_eq!(
            fs::read_to_string(p.join("src/A.java")).unwrap(),
            "class A {}"
        );
        assert_eq!(
            fs::read_to_string(p.join("src/B.java")).unwrap(),
            "class B {}"
        );
        assert!(!p.join("src/new").exists());
        // skipped dirs are left alone
        assert_eq!(
            fs::read_to_string(p.join("build/out.class")).unwrap(),
            "binary"
        );
    }

    #[test]
    fn snapshot_persists_recovery_manifest_before_returning() {
        let proj = tempdir().unwrap();
        let snaps = tempdir().unwrap();
        write(&proj.path().join("src/A.java"), "class A {}");
        let session_id = Uuid::new_v4();
        let mut store = CheckpointStore::new();

        let checkpoint = store
            .snapshot(session_id, "pre-transform", proj.path(), snaps.path())
            .unwrap();

        let persisted: CheckpointStore =
            serde_json::from_slice(&fs::read(snaps.path().join("checkpoints.json")).unwrap())
                .unwrap();
        assert_eq!(persisted.checkpoints.len(), 1);
        assert_eq!(persisted.checkpoints[0].id, checkpoint.id);
        assert_eq!(persisted.checkpoints[0].session_id, session_id);
        assert_eq!(
            fs::read_to_string(checkpoint.snapshot_path.join("src/A.java")).unwrap(),
            "class A {}"
        );
    }

    #[test]
    fn interrupted_session_can_be_restored_from_persisted_manifest() {
        let proj = tempdir().unwrap();
        let snaps = tempdir().unwrap();
        write(&proj.path().join("src/A.java"), "class Original {}");
        let session_id = Uuid::new_v4();
        let mut store = CheckpointStore::new();
        store
            .snapshot(session_id, "pre-transform", proj.path(), snaps.path())
            .unwrap();

        write(&proj.path().join("src/A.java"), "class PartiallyChanged {}");
        write(&proj.path().join("src/NEW.java"), "class NewFile {}");

        let persisted: CheckpointStore =
            serde_json::from_slice(&fs::read(snaps.path().join("checkpoints.json")).unwrap())
                .unwrap();
        let checkpoint = persisted
            .checkpoints
            .iter()
            .find(|item| item.session_id == session_id)
            .unwrap();
        persisted.restore(checkpoint.id, proj.path()).unwrap();

        assert_eq!(
            fs::read_to_string(proj.path().join("src/A.java")).unwrap(),
            "class Original {}"
        );
        assert!(!proj.path().join("src/NEW.java").exists());
    }

    #[test]
    fn test_snapshot_inside_project_rejected() {
        let proj = tempdir().unwrap();
        let mut store = CheckpointStore::new();
        let err = store
            .snapshot(Uuid::new_v4(), "x", proj.path(), &proj.path().join(".ummp"))
            .unwrap_err();
        assert!(matches!(err, CheckpointError::InvalidLocation(_)));
    }

    #[test]
    fn test_restore_unknown_id() {
        let proj = tempdir().unwrap();
        let store = CheckpointStore::new();
        let err = store.restore(Uuid::new_v4(), proj.path()).unwrap_err();
        assert!(matches!(err, CheckpointError::NotFound(_)));
    }
}
