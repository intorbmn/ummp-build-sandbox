//! `ummp port` (Phase 9): a user's mod source in, the ported source out.
//!
//! The input (folder or `.zip`) is never modified: it is copied to a new output folder and
//! the full pipeline (version migration, adapters, loader conversion) runs on the copy
//! through `MigrationEngine`. The copy gets `UMMP_REPORT.md`; `zip` also writes `<output>.zip`.

use crate::analyzer::AnalyzerEngine;
use crate::build::local::copy_project;
use crate::checkpoint::{collect_files, CheckpointStore};
use crate::filesystem::{resolve_within, FileSystem, LocalFileSystem};
use crate::knowledge::KnowledgeBase;
use crate::metadata::Loader;
use crate::migration::{
    MigrationEngine, MigrationError, MigrationPlanner, MigrationReport, MigrationRequest,
    RunOptions,
};
use crate::project::{DefaultProjectManager, ProjectManager};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PortOptions {
    /// Source folder or `.zip` of one.
    pub input: PathBuf,
    /// Output folder; default: `<default_port_root>/<mod>-<mc>-<loader>-<time>` (see [`default_port_root`]).
    pub output: Option<PathBuf>,
    /// Target Minecraft version.
    pub to: String,
    /// Target loader; default: the mod's current loader.
    pub loader: Option<String>,
    /// Source Minecraft version; default: detected from the descriptor.
    pub from: Option<String>,
    /// Also write `<output>.zip`.
    pub zip: bool,
    pub min_confidence: f64,
}

#[derive(Debug)]
pub struct PortResult {
    pub output: PathBuf,
    pub zip: Option<PathBuf>,
    pub report: MigrationReport,
}

#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Migration(#[from] MigrationError),
    #[error("{0}")]
    Input(String),
}

/// `D:\Claude\ummp-ports` on Windows, `~/ummp-ports` elsewhere.
#[must_use]
pub fn default_port_root() -> PathBuf {
    if cfg!(windows) {
        return PathBuf::from(r"D:\Claude\ummp-ports");
    }
    std::env::var_os("HOME")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join("ummp-ports")
}

/// Extracts `zip` into `dir`, refusing entries that would land outside it.
fn extract(zip: &Path, dir: &Path) -> Result<(), PortError> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip)?)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let safe = entry
            .enclosed_name()
            .map(Path::to_path_buf)
            .and_then(|rel| resolve_within(dir, &rel).ok())
            .ok_or_else(|| PortError::Input(format!("unsafe path in zip: {name}")))?;
        if entry.is_dir() {
            std::fs::create_dir_all(&safe)?;
        } else {
            if let Some(parent) = safe.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::io::copy(&mut entry, &mut std::fs::File::create(&safe)?)?;
        }
    }
    Ok(())
}

/// A zip with one top-level folder is rooted at that folder.
fn single_root(dir: &Path) -> std::io::Result<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    match entries.as_slice() {
        [only] if only.file_type()?.is_dir() => Ok(only.path()),
        _ => Ok(dir.to_path_buf()),
    }
}

const fn loader_name(l: &Loader) -> Option<&'static str> {
    match l {
        Loader::Forge => Some("forge"),
        Loader::Fabric => Some("fabric"),
        Loader::NeoForge => Some("neoforge"),
        Loader::Quilt => Some("quilt"),
        _ => None,
    }
}

fn write_zip(root: &Path, zip: &Path) -> Result<(), PortError> {
    let mut z = zip::ZipWriter::new(std::fs::File::create(zip)?);
    let options = zip::write::FileOptions::default();
    for rel in collect_files(root)? {
        z.start_file(rel.to_string_lossy().replace('\\', "/"), options)?;
        z.write_all(&std::fs::read(root.join(&rel))?)?;
    }
    z.finish()?;
    Ok(())
}

fn safe_name(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// The folder to read a mod from: `input` itself, or `input` (a zip) extracted into
/// `staging`. Shared by `port` and the application services' plan preview.
///
/// # Errors
/// Returns an error if the zip cannot be extracted or has no single root.
pub fn stage_source(input: &Path, staging: &Path) -> Result<PathBuf, PortError> {
    if input.is_dir() {
        Ok(input.to_path_buf())
    } else {
        extract(input, staging)?;
        Ok(single_root(staging)?)
    }
}

/// Ports `opts.input` into a new output folder. See the module docs.
///
/// # Errors
/// Bad input (unsafe zip, unknown versions), a non-empty output folder, I/O, or migration
/// errors.
pub fn port(opts: &PortOptions, kb: &KnowledgeBase) -> Result<PortResult, PortError> {
    if let Some(out) = &opts.output {
        if out.exists() && std::fs::read_dir(out)?.next().is_some() {
            return Err(PortError::Input(format!(
                "output folder {} is not empty",
                out.display()
            )));
        }
    }
    let staging = tempfile::tempdir()?;
    let source = stage_source(&opts.input, staging.path())?;

    let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
    let first = AnalyzerEngine::default()
        .analyze(&source, &fs)
        .ok()
        .and_then(|r| r.mods.into_iter().next());
    let source_loader = first
        .as_ref()
        .and_then(|m| loader_name(&m.metadata.loader))
        .map(String::from);
    let from = opts
        .from
        .clone()
        .or_else(|| first.as_ref().map(|m| m.metadata.minecraft.to_string()))
        .ok_or_else(|| {
            PortError::Input("cannot detect the Minecraft version: pass --from".into())
        })?;
    let loader = opts
        .loader
        .clone()
        .or_else(|| source_loader.clone())
        .ok_or_else(|| PortError::Input("cannot detect the loader: pass --loader".into()))?;
    let mod_name = first
        .as_ref()
        .map(|m| m.metadata.mod_id.clone())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| "mod".into());

    let output = opts.output.clone().unwrap_or_else(|| {
        default_port_root().join(safe_name(&format!(
            "{mod_name}-{}-{loader}-{}",
            opts.to,
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        )))
    });
    std::fs::create_dir_all(&output)?;
    copy_project(&source, &output)?;

    let plan = MigrationPlanner::plan(
        kb,
        &MigrationRequest {
            source_minecraft: from,
            target_minecraft: opts.to.clone(),
            target_loader: loader,
            min_confidence: opts.min_confidence,
            source_loader,
        },
    )?;
    let project = DefaultProjectManager
        .open(&output)
        .map_err(|e| PortError::Input(e.to_string()))?;
    let snapshots = tempfile::tempdir()?;
    let report = MigrationEngine::default().run(
        &plan,
        &project,
        &mut CheckpointStore::new(),
        &RunOptions {
            dry_run: false,
            snapshot_root: snapshots.path(),
            session_id: uuid::Uuid::new_v4(),
        },
    )?;
    std::fs::write(
        output.join("UMMP_REPORT.md"),
        format!(
            "<!-- Ported by UMMP from {}. The original was not modified. -->\n\n{}",
            opts.input.display(),
            report.to_markdown()
        ),
    )?;
    let zip = if opts.zip {
        let mut name = output.file_name().unwrap_or_default().to_os_string();
        name.push(".zip");
        let path = output.with_file_name(name);
        write_zip(&output, &path)?;
        Some(path)
    } else {
        None
    };
    Ok(PortResult {
        output,
        zip,
        report,
    })
}
