//! Phase 10 — modpack migration (ADR 0013).
//!
//! A [`Pack`] is a modpack read from one of four formats (Modrinth `.mrpack`, CurseForge zip,
//! Prism/MultiMC instance, plain folder). Entries may or may not have a local JAR: mrpack
//! files are listed by URL and hash, CurseForge files by ids. Everything a pack names is
//! untrusted: paths go through [`safe_rel`], archive sizes are capped on the bytes read.

pub mod analyze;
pub mod config;
pub mod formats;
pub mod graph;
pub mod plan;
pub mod report;
pub mod run;

use crate::analyzer::ModAnalysis;
use crate::metadata::Loader;
use crate::scan::{Finding, Severity};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha512};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("unsafe path in pack: {0}")]
    UnsafePath(String),
    #[error("hash mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("limit exceeded: {0}")]
    LimitExceeded(String),
    #[error("invalid pack: {0}")]
    Invalid(String),
    #[error("unsupported pack: {0}")]
    Unsupported(String),
    #[error("refused: {0}")]
    Refused(String),
    #[error("mod index: {0}")]
    Index(String),
    #[error(transparent)]
    Migration(#[from] crate::migration::MigrationError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackFormat {
    Mrpack,
    CurseForge,
    Prism,
    Folder,
}

/// Where a file is needed, from the mrpack `env` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    #[default]
    Required,
    Optional,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Env {
    pub client: Support,
    pub server: Support,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// A JAR under `mods/`: takes part in analysis, graph and plan.
    Mod,
    /// Any other listed file (resource pack, shader pack): passed through.
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackEntry {
    /// Path inside the instance, always `/`-separated and safe (e.g. `mods/x.jar`).
    pub path: String,
    pub kind: EntryKind,
    pub sha1: Option<String>,
    pub sha512: Option<String>,
    pub size: Option<u64>,
    pub env: Env,
    pub downloads: Vec<String>,
    /// The JAR on disk, when the pack carries it.
    pub local: Option<PathBuf>,
    /// CurseForge `(projectID, fileID)`; such an entry has no JAR and no hash.
    pub curseforge: Option<(u64, u64)>,
    /// Filled by [`analyze::analyze_pack`].
    pub mod_info: Option<ModAnalysis>,
    pub risk: Option<Severity>,
    pub findings: Vec<Finding>,
    pub technologies: Vec<String>,
}

impl PackEntry {
    #[must_use]
    pub fn new(path: &str, kind: EntryKind) -> Self {
        Self {
            path: path.to_string(),
            kind,
            sha1: None,
            sha512: None,
            size: None,
            env: Env::default(),
            downloads: Vec::new(),
            local: None,
            curseforge: None,
            mod_info: None,
            risk: None,
            findings: Vec::new(),
            technologies: Vec::new(),
        }
    }

    /// A readable id for reports when no descriptor is available.
    #[must_use]
    pub fn display_id(&self) -> String {
        if let Some(m) = &self.mod_info {
            return m.metadata.mod_id.clone();
        }
        if let Some((p, f)) = self.curseforge {
            return format!("curseforge:{p}/{f}");
        }
        Path::new(&self.path)
            .file_stem()
            .map_or_else(|| self.path.clone(), |s| s.to_string_lossy().into_owned())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Pack {
    pub name: String,
    pub version: String,
    pub format: PackFormat,
    pub mc: Option<String>,
    pub loader: Option<Loader>,
    pub loader_version: Option<String>,
    /// Directory the relative paths below refer to (the extracted pack, or the instance folder).
    pub root: PathBuf,
    pub entries: Vec<PackEntry>,
    /// Non-mod files to carry over (configs, scripts, …), relative to `root`.
    pub overrides: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

/// Caps for untrusted archives. Enforced on the bytes actually read, not on header sizes.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
    pub max_entries: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entry_bytes: 256 * 1024 * 1024,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            max_entries: 20_000,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub limits: Limits,
    /// For plain folders: override what the JARs say.
    pub from_mc: Option<String>,
    pub from_loader: Option<Loader>,
}

/// Imports a pack, extracting archives into `work` (which must be empty or absent).
///
/// # Errors
/// Unsafe paths, hash mismatches, size limits, malformed or unsupported packs, I/O.
pub fn import(path: &Path, work: &Path) -> Result<Pack, PackError> {
    import_with(path, work, &ImportOptions::default())
}

/// [`import`] with explicit options.
///
/// # Errors
/// See [`import`].
pub fn import_with(path: &Path, work: &Path, opts: &ImportOptions) -> Result<Pack, PackError> {
    if path.is_dir() {
        return if path.join("mmc-pack.json").is_file() {
            formats::read_instance(path, opts)
        } else {
            formats::read_folder(path, opts)
        };
    }
    let archive = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let names: Vec<&str> = archive.file_names().collect();
    if names.contains(&"modrinth.index.json") {
        formats::read_mrpack(archive, work, opts)
    } else if names.contains(&"manifest.json") {
        formats::read_curseforge(archive, work, opts)
    } else {
        Err(PackError::Unsupported(format!(
            "{}: not a .mrpack, a CurseForge zip, an instance or a folder",
            path.display()
        )))
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Validates a path taken from a pack: relative, `/`-separated, no `..`, no drive letters.
///
/// # Errors
/// [`PackError::UnsafePath`] for anything that could leave the instance root.
pub fn safe_rel(raw: &str) -> Result<PathBuf, PackError> {
    let bad = || PackError::UnsafePath(raw.to_string());
    if raw.is_empty()
        || raw.contains('\0')
        || raw.contains('\\')
        || raw.contains(':')
        || raw.starts_with('/')
    {
        return Err(bad());
    }
    let mut out = PathBuf::new();
    for seg in raw.split('/') {
        match seg {
            "" | "." => {}
            ".." => return Err(bad()),
            s => out.push(s),
        }
    }
    if out.as_os_str().is_empty() {
        return Err(bad());
    }
    crate::filesystem::resolve_within(Path::new("/x"), &out).map_err(|_| bad())?;
    Ok(out)
}

/// `root` joined with a validated pack path.
///
/// # Errors
/// See [`safe_rel`].
pub fn join_safe(root: &Path, raw: &str) -> Result<PathBuf, PackError> {
    let rel = safe_rel(raw)?;
    crate::filesystem::resolve_within(root, &rel).map_err(|_| PackError::UnsafePath(raw.into()))
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[must_use]
pub fn sha1_hex(bytes: &[u8]) -> String {
    to_hex(&Sha1::digest(bytes))
}

#[must_use]
pub fn sha512_hex(bytes: &[u8]) -> String {
    to_hex(&Sha512::digest(bytes))
}

/// Relative paths (`/`-separated) of all regular files under `root`, sorted. Symlinks are skipped.
pub(crate) fn walk_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    fn go(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let ft = e.file_type()?;
            if ft.is_symlink() {
                continue;
            }
            let p = e.path();
            if ft.is_dir() {
                go(root, &p, out)?;
            } else if ft.is_file() {
                if let Ok(rel) = p.strip_prefix(root) {
                    out.push(rel.to_path_buf());
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    if root.is_dir() {
        go(root, root, &mut out)?;
    }
    out.sort();
    Ok(out)
}

pub(crate) fn rel_string(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// `1.21.0` → `1.21`, so versions print the way Minecraft names them.
pub(crate) fn fmt_mc(v: &crate::metadata::MinecraftVersion) -> String {
    if v.patch == 0 {
        format!("{}.{}", v.major, v.minor)
    } else {
        format!("{}.{}.{}", v.major, v.minor, v.patch)
    }
}
