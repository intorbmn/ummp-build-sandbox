//! Small helpers the CLI used to own; now shared with the UI.

use super::AppError;
use crate::analyzer::AnalyzerEngine;
use crate::build_script::required_java;
use crate::filesystem::{FileSystem, LocalFileSystem};
use crate::knowledge::KnowledgeBase;
use crate::metadata::Loader;
use crate::modpack::plan::Side;
use crate::modpack::PackFormat;
use crate::scan::Severity;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Opens the knowledge base: a SQLite file, a mappings JSON loaded into memory, or an
/// empty in-memory one (structural migration only).
///
/// # Errors
/// Returns an error when the database or the mappings file cannot be read.
pub fn open_kb(kb: Option<PathBuf>, mappings: Option<PathBuf>) -> Result<KnowledgeBase, AppError> {
    Ok(match (kb, mappings) {
        (Some(db), _) => KnowledgeBase::open(&db)?,
        (None, Some(file)) => {
            let mem = KnowledgeBase::open(Path::new(":memory:"))?;
            mem.import_mappings_from_file(&file)?;
            mem
        }
        (None, None) => KnowledgeBase::open(Path::new(":memory:"))?,
    })
}

/// What a build request must do about the scan result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskDecision {
    Allow,
    /// Not local: runs on a disposable runner; tell the person, then go on.
    AllowWithWarning,
    /// Medium risk on this machine: ask first (the CLI prompts, the UI shows a checkbox).
    Confirm,
    Refuse(String),
}

/// Local builds run mod code on this machine: High risk needs `accept`, Medium needs
/// `yes` or `accept`. GitHub builds run on a disposable runner and only warn.
#[must_use]
pub fn risk_decision(risk: Option<Severity>, local: bool, accept: bool, yes: bool) -> RiskDecision {
    if !local {
        return if risk.is_some() {
            RiskDecision::AllowWithWarning
        } else {
            RiskDecision::Allow
        };
    }
    match risk {
        Some(Severity::High) if !accept => RiskDecision::Refuse(
            "refusing to build a High-risk mod on this machine. \
             Build on GitHub (drop --local), or review the findings and pass --i-accept-risk"
                .into(),
        ),
        Some(Severity::Medium) if !yes && !accept => RiskDecision::Confirm,
        _ => RiskDecision::Allow,
    }
}

fn first_mod(path: &Path) -> Option<crate::analyzer::ModAnalysis> {
    let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
    AnalyzerEngine::default()
        .analyze(path, &fs)
        .ok()
        .and_then(|r| r.mods.into_iter().next())
}

/// A folder- and branch-safe mod name, and the Java version the mod needs, if known.
#[must_use]
pub fn mod_identity(path: &Path) -> (String, Option<u32>) {
    let first = first_mod(path);
    let raw = first
        .as_ref()
        .map(|m| m.metadata.mod_id.clone())
        .filter(|id| !id.is_empty())
        .or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "mod".into());
    let name: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    let java = first.and_then(|m| {
        m.detected_java
            .map(|j| j.major)
            .or_else(|| m.metadata.java.map(|j| j.major))
            .or_else(|| required_java(&m.metadata.minecraft.to_string()))
    });
    (if name.is_empty() { "mod".into() } else { name }, java)
}

/// Lowercase loader name (`fabric`, `neoforge`, ...), if it is one UMMP converts.
#[must_use]
pub const fn loader_label(loader: &Loader) -> Option<&'static str> {
    match loader {
        Loader::Forge => Some("forge"),
        Loader::Fabric => Some("fabric"),
        Loader::NeoForge => Some("neoforge"),
        Loader::Quilt => Some("quilt"),
        _ => None,
    }
}

/// The loader named in the project's descriptor, lowercase.
#[must_use]
pub fn detected_loader(path: &Path) -> Option<String> {
    first_mod(path).and_then(|m| loader_label(&m.metadata.loader).map(str::to_owned))
}

/// What a user-chosen path is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Folder,
    Zip,
    Jar,
}

/// Checks that `path` exists and is a folder, a `.zip`, or (when `allow_jar`) a `.jar`.
///
/// # Errors
/// Returns [`AppError::Input`] with a sentence that says what to choose instead.
pub fn check_input(path: &Path, allow_jar: bool) -> Result<InputKind, AppError> {
    if !path.exists() {
        return Err(AppError::Input(format!(
            "{} does not exist. Choose a mod folder or a .zip file.",
            path.display()
        )));
    }
    if path.is_dir() {
        return Ok(InputKind::Folder);
    }
    match path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("zip") => Ok(InputKind::Zip),
        Some("jar") if allow_jar => Ok(InputKind::Jar),
        Some("jar") => Err(AppError::Input(format!(
            "{} is a compiled JAR; it can be scanned, but analysis and porting need the source folder or .zip.",
            path.display()
        ))),
        _ => Err(AppError::Input(format!(
            "{} is not a folder or a .zip file.",
            path.display()
        ))),
    }
}

/// # Errors
/// Returns [`AppError::Input`] for an unknown format name.
pub fn pack_format(s: &str) -> Result<PackFormat, AppError> {
    match s {
        "mrpack" => Ok(PackFormat::Mrpack),
        "curseforge" => Ok(PackFormat::CurseForge),
        "prism" => Ok(PackFormat::Prism),
        "folder" => Ok(PackFormat::Folder),
        other => Err(AppError::Input(format!(
            "unknown pack format {other:?} (mrpack, curseforge, prism, folder)"
        ))),
    }
}

/// # Errors
/// Returns [`AppError::Input`] for an unknown side name.
pub fn side(s: &str) -> Result<Side, AppError> {
    match s {
        "client" => Ok(Side::Client),
        "server" => Ok(Side::Server),
        "both" => Ok(Side::Both),
        other => Err(AppError::Input(format!(
            "unknown side {other:?} (client, server, both)"
        ))),
    }
}

/// Index used offline: the planner never calls it, and if it did it would find nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoIndex;

impl crate::modpack::plan::ModIndex for NoIndex {
    fn versions(
        &self,
        _: &str,
        _: &str,
        _: &crate::metadata::Loader,
    ) -> Result<Vec<crate::modpack::plan::RemoteVersion>, crate::modpack::PackError> {
        Ok(Vec::new())
    }

    fn by_hash(
        &self,
        _: &str,
    ) -> Result<Option<crate::modpack::plan::RemoteVersion>, crate::modpack::PackError> {
        Ok(None)
    }
}
