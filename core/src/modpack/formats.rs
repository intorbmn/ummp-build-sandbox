//! Readers (and, from 10.F, writers) for the four pack formats.

use super::{
    fmt_mc, join_safe, rel_string, safe_rel, sha1_hex, sha512_hex, walk_files, EntryKind, Env,
    ImportOptions, Limits, Pack, PackEntry, PackError, PackFormat, Support,
};
use crate::analyzer::inspect_jar;
use crate::filesystem::LocalFileSystem;
use crate::metadata::Loader;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

/// Download hosts the mrpack spec allows. Data, not logic: add a host here.
pub const ALLOWED_DOWNLOAD_HOSTS: &[&str] = &[
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "gitlab.com",
];

/// Cap for the small JSON/INI descriptors of a pack.
const MAX_DESCRIPTOR_BYTES: u64 = 8 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Archive helpers
// ---------------------------------------------------------------------------

/// Running total of bytes extracted from one archive.
struct Budget {
    limits: Limits,
    total: u64,
}

fn check_archive(archive: &ZipArchive<File>, limits: &Limits) -> Result<(), PackError> {
    if archive.len() > limits.max_entries {
        return Err(PackError::LimitExceeded(format!(
            "archive has {} entries (limit {})",
            archive.len(),
            limits.max_entries
        )));
    }
    Ok(())
}

fn read_small(archive: &mut ZipArchive<File>, name: &str) -> Result<Vec<u8>, PackError> {
    let entry = archive
        .by_name(name)
        .map_err(|_| PackError::Invalid(format!("missing {name}")))?;
    let mut buf = Vec::new();
    entry.take(MAX_DESCRIPTOR_BYTES + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_DESCRIPTOR_BYTES {
        return Err(PackError::LimitExceeded(format!("{name} is too large")));
    }
    Ok(buf)
}

/// Copies one archive entry to `dest`, counting the bytes really read.
fn copy_capped(
    entry: &mut zip::read::ZipFile<'_>,
    dest: &Path,
    name: &str,
    budget: &mut Budget,
) -> Result<Vec<u8>, PackError> {
    let mut buf = Vec::new();
    entry
        .take(budget.limits.max_entry_bytes + 1)
        .read_to_end(&mut buf)?;
    if buf.len() as u64 > budget.limits.max_entry_bytes {
        return Err(PackError::LimitExceeded(format!(
            "{name} exceeds {} bytes",
            budget.limits.max_entry_bytes
        )));
    }
    budget.total += buf.len() as u64;
    if budget.total > budget.limits.max_total_bytes {
        return Err(PackError::LimitExceeded(format!(
            "archive expands past {} bytes",
            budget.limits.max_total_bytes
        )));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, &buf)?;
    Ok(buf)
}

/// Extracts every entry under `prefix` (e.g. `overrides/`) into `root`.
fn extract_prefix(
    archive: &mut ZipArchive<File>,
    prefix: &str,
    root: &Path,
    budget: &mut Budget,
    warnings: &mut Vec<String>,
) -> Result<(), PackError> {
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let Some(rest) = name.strip_prefix(prefix) else {
            continue;
        };
        if rest.is_empty() || entry.is_dir() {
            continue;
        }
        let dest = join_safe(root, rest)?;
        if dest.exists() && prefix != "overrides/" {
            warnings.push(format!("{prefix}{rest} overwrites a file from overrides/"));
        }
        copy_capped(&mut entry, &dest, &name, budget)?;
    }
    Ok(())
}

/// Names of entries under `prefix`, checked before anything is written.
fn validate_prefix_names(archive: &ZipArchive<File>, prefix: &str) -> Result<(), PackError> {
    for name in archive.file_names() {
        if let Some(rest) = name.strip_prefix(prefix) {
            if !rest.is_empty() && !name.ends_with('/') {
                safe_rel(rest)?;
            }
        }
    }
    Ok(())
}

fn prepare_work(work: &Path) -> Result<(), PackError> {
    if work.exists() && std::fs::read_dir(work)?.next().is_some() {
        return Err(PackError::Invalid(format!(
            "work folder {} is not empty",
            work.display()
        )));
    }
    std::fs::create_dir_all(work)?;
    Ok(())
}

fn is_mod_jar(rel: &str) -> bool {
    rel.starts_with("mods/") && rel.to_ascii_lowercase().ends_with(".jar")
}

/// Turns the files under `root` into entries (JARs under `mods/`) and overrides (the rest).
fn collect_local(pack: &mut Pack) -> Result<(), PackError> {
    for rel in walk_files(&pack.root)? {
        let s = rel_string(&rel);
        if pack.entries.iter().any(|e| e.path == s) {
            continue;
        }
        if is_mod_jar(&s) {
            let abs = pack.root.join(&rel);
            let bytes = std::fs::read(&abs)?;
            let mut e = PackEntry::new(&s, EntryKind::Mod);
            e.sha1 = Some(sha1_hex(&bytes));
            e.sha512 = Some(sha512_hex(&bytes));
            e.size = Some(bytes.len() as u64);
            e.local = Some(abs);
            pack.entries.push(e);
        } else {
            pack.overrides.push(rel);
        }
    }
    pack.entries.sort_by(|a, b| a.path.cmp(&b.path));
    pack.overrides.sort();
    Ok(())
}

// ---------------------------------------------------------------------------
// Modrinth .mrpack
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MrIndex {
    #[serde(rename = "formatVersion")]
    format_version: u32,
    game: String,
    #[serde(rename = "versionId", default)]
    version_id: String,
    name: String,
    #[serde(default)]
    files: Vec<MrFile>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct MrFile {
    path: String,
    hashes: MrHashes,
    env: Option<MrEnv>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(rename = "fileSize")]
    file_size: Option<u64>,
}

#[derive(Deserialize)]
struct MrHashes {
    sha1: String,
    sha512: String,
}

#[derive(Deserialize)]
struct MrEnv {
    client: Option<String>,
    server: Option<String>,
}

fn support(v: Option<&str>) -> Result<Support, PackError> {
    match v {
        None | Some("required") => Ok(Support::Required),
        Some("optional") => Ok(Support::Optional),
        Some("unsupported") => Ok(Support::Unsupported),
        Some(o) => Err(PackError::Invalid(format!("unknown env value {o:?}"))),
    }
}

fn check_hex(path: &str, what: &str, v: &str, len: usize) -> Result<String, PackError> {
    if v.len() != len || !v.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(PackError::Invalid(format!(
            "{path}: {what} must be {len} hex characters"
        )));
    }
    Ok(v.to_ascii_lowercase())
}

/// The host of an `https://` URL, or `None` for any other scheme.
fn https_host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let end = rest.find(['/', ':', '?', '#']).unwrap_or(rest.len());
    Some(&rest[..end])
}

fn loader_from_mrpack(deps: &BTreeMap<String, String>) -> (Option<Loader>, Option<String>) {
    for (key, loader) in [
        ("fabric-loader", Loader::Fabric),
        ("quilt-loader", Loader::Quilt),
        ("neoforge", Loader::NeoForge),
        ("forge", Loader::Forge),
    ] {
        if let Some(v) = deps.get(key) {
            return (Some(loader), Some(v.clone()));
        }
    }
    (None, None)
}

pub fn read_mrpack(
    mut archive: ZipArchive<File>,
    work: &Path,
    opts: &ImportOptions,
) -> Result<Pack, PackError> {
    check_archive(&archive, &opts.limits)?;
    let raw = read_small(&mut archive, "modrinth.index.json")?;
    let index: MrIndex = serde_json::from_slice(&raw)
        .map_err(|e| PackError::Invalid(format!("modrinth.index.json: {e}")))?;
    if index.format_version != 1 || index.game != "minecraft" {
        return Err(PackError::Unsupported(format!(
            "mrpack formatVersion {} game {:?}",
            index.format_version, index.game
        )));
    }

    // Validate every path and hash before anything is written.
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for f in &index.files {
        let rel = safe_rel(&f.path)?;
        let path = rel_string(&rel);
        let kind = if is_mod_jar(&path) {
            EntryKind::Mod
        } else {
            EntryKind::Other
        };
        let mut e = PackEntry::new(&path, kind);
        e.sha1 = Some(check_hex(&path, "sha1", &f.hashes.sha1, 40)?);
        e.sha512 = Some(check_hex(&path, "sha512", &f.hashes.sha512, 128)?);
        e.size = f.file_size;
        e.env = Env {
            client: support(f.env.as_ref().and_then(|v| v.client.as_deref()))?,
            server: support(f.env.as_ref().and_then(|v| v.server.as_deref()))?,
        };
        for url in &f.downloads {
            match https_host(url) {
                Some(h) if ALLOWED_DOWNLOAD_HOSTS.contains(&h) => {}
                Some(h) => warnings.push(format!(
                    "{path}: download host {h} is not on the mrpack allow-list"
                )),
                None => warnings.push(format!("{path}: download URL {url} is not https")),
            }
        }
        e.downloads.clone_from(&f.downloads);
        entries.push(e);
    }
    for prefix in ["overrides/", "client-overrides/", "server-overrides/"] {
        validate_prefix_names(&archive, prefix)?;
    }

    prepare_work(work)?;
    let mut budget = Budget {
        limits: opts.limits,
        total: 0,
    };
    // A file the zip also bundles at its listed path must match its hashes.
    for e in &mut entries {
        let Ok(mut z) = archive.by_name(&e.path) else {
            continue;
        };
        let dest = join_safe(work, &e.path)?;
        let bytes = copy_capped(&mut z, &dest, &e.path.clone(), &mut budget)?;
        for (want, got) in [
            (e.sha1.clone().unwrap_or_default(), sha1_hex(&bytes)),
            (e.sha512.clone().unwrap_or_default(), sha512_hex(&bytes)),
        ] {
            if want != got {
                return Err(PackError::HashMismatch {
                    path: e.path.clone(),
                    expected: want,
                    actual: got,
                });
            }
        }
        e.local = Some(dest);
    }
    for prefix in ["overrides/", "client-overrides/", "server-overrides/"] {
        extract_prefix(&mut archive, prefix, work, &mut budget, &mut warnings)?;
    }

    let (loader, loader_version) = loader_from_mrpack(&index.dependencies);
    let mut pack = Pack {
        name: index.name,
        version: index.version_id,
        format: PackFormat::Mrpack,
        mc: index.dependencies.get("minecraft").cloned(),
        loader,
        loader_version,
        root: work.to_path_buf(),
        entries,
        overrides: Vec::new(),
        warnings,
    };
    collect_local(&mut pack)?;
    Ok(pack)
}

// ---------------------------------------------------------------------------
// CurseForge zip
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CfManifest {
    minecraft: CfMinecraft,
    #[serde(rename = "manifestType")]
    manifest_type: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    files: Vec<CfFile>,
    overrides: Option<String>,
}

#[derive(Deserialize)]
struct CfMinecraft {
    version: String,
    #[serde(rename = "modLoaders", default)]
    mod_loaders: Vec<CfLoader>,
}

#[derive(Deserialize)]
struct CfLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct CfFile {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default = "yes")]
    required: bool,
}

const fn yes() -> bool {
    true
}

fn loader_by_name(name: &str) -> Option<Loader> {
    match name {
        "forge" => Some(Loader::Forge),
        "neoforge" => Some(Loader::NeoForge),
        "fabric" => Some(Loader::Fabric),
        "quilt" => Some(Loader::Quilt),
        _ => None,
    }
}

pub fn read_curseforge(
    mut archive: ZipArchive<File>,
    work: &Path,
    opts: &ImportOptions,
) -> Result<Pack, PackError> {
    check_archive(&archive, &opts.limits)?;
    let raw = read_small(&mut archive, "manifest.json")?;
    let m: CfManifest = serde_json::from_slice(&raw)
        .map_err(|e| PackError::Invalid(format!("manifest.json: {e}")))?;
    if m.manifest_type != "minecraftModpack" {
        return Err(PackError::Unsupported(format!(
            "manifestType {:?}",
            m.manifest_type
        )));
    }
    let overrides_dir = m.overrides.unwrap_or_else(|| "overrides".into());
    let prefix = format!("{}/", rel_string(&safe_rel(&overrides_dir)?));
    validate_prefix_names(&archive, &prefix)?;

    let primary = m
        .minecraft
        .mod_loaders
        .iter()
        .find(|l| l.primary)
        .or_else(|| m.minecraft.mod_loaders.first());
    let (loader, loader_version) = primary
        .and_then(|l| l.id.split_once('-'))
        .map_or((None, None), |(n, v)| {
            (loader_by_name(n), Some(v.to_string()))
        });

    prepare_work(work)?;
    let mut budget = Budget {
        limits: opts.limits,
        total: 0,
    };
    let mut warnings = Vec::new();
    extract_prefix(&mut archive, &prefix, work, &mut budget, &mut warnings)?;

    let mut entries: Vec<PackEntry> = m
        .files
        .iter()
        .map(|f| {
            let mut e = PackEntry::new(
                &format!("mods/curseforge-{}-{}.jar", f.project_id, f.file_id),
                EntryKind::Mod,
            );
            e.curseforge = Some((f.project_id, f.file_id));
            if !f.required {
                e.env = Env {
                    client: Support::Optional,
                    server: Support::Optional,
                };
            }
            e
        })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut pack = Pack {
        name: m.name,
        version: m.version,
        format: PackFormat::CurseForge,
        mc: Some(m.minecraft.version),
        loader,
        loader_version,
        root: work.to_path_buf(),
        entries,
        overrides: Vec::new(),
        warnings,
    };
    collect_local(&mut pack)?;
    Ok(pack)
}

// ---------------------------------------------------------------------------
// Prism / MultiMC instance
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MmcPack {
    #[serde(default)]
    components: Vec<MmcComponent>,
}

#[derive(Deserialize)]
struct MmcComponent {
    uid: String,
    #[serde(default)]
    version: Option<String>,
}

pub fn read_instance(dir: &Path, opts: &ImportOptions) -> Result<Pack, PackError> {
    let raw = std::fs::read(dir.join("mmc-pack.json"))?;
    let mmc: MmcPack = serde_json::from_slice(&raw)
        .map_err(|e| PackError::Invalid(format!("mmc-pack.json: {e}")))?;
    let mut mc = None;
    let mut loader = None;
    let mut loader_version = None;
    for c in &mmc.components {
        match c.uid.as_str() {
            "net.minecraft" => mc.clone_from(&c.version),
            "net.minecraftforge" => {
                (loader, loader_version) = (Some(Loader::Forge), c.version.clone());
            }
            "net.neoforged" => {
                (loader, loader_version) = (Some(Loader::NeoForge), c.version.clone());
            }
            "net.fabricmc.fabric-loader" => {
                (loader, loader_version) = (Some(Loader::Fabric), c.version.clone());
            }
            "org.quiltmc.quilt-loader" => {
                (loader, loader_version) = (Some(Loader::Quilt), c.version.clone());
            }
            _ => {}
        }
    }
    let name = std::fs::read_to_string(dir.join("instance.cfg"))
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("name=").map(|v| v.trim().to_string()))
        })
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let root = [".minecraft", "minecraft"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_dir())
        .ok_or_else(|| {
            PackError::Invalid("instance has no .minecraft or minecraft folder".into())
        })?;
    let mut pack = Pack {
        name,
        version: String::new(),
        format: PackFormat::Prism,
        mc,
        loader,
        loader_version,
        root,
        entries: Vec::new(),
        overrides: Vec::new(),
        warnings: Vec::new(),
    };
    collect_local(&mut pack)?;
    apply_detection(&mut pack, opts);
    Ok(pack)
}

// ---------------------------------------------------------------------------
// Plain folder
// ---------------------------------------------------------------------------

pub fn read_folder(dir: &Path, opts: &ImportOptions) -> Result<Pack, PackError> {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut pack = Pack {
        name,
        version: String::new(),
        format: PackFormat::Folder,
        mc: None,
        loader: None,
        loader_version: None,
        root: dir.to_path_buf(),
        entries: Vec::new(),
        overrides: Vec::new(),
        warnings: Vec::new(),
    };
    if dir.join("mods").is_dir() {
        collect_local(&mut pack)?;
    } else {
        // A bare mods folder: JARs directly inside it.
        for rel in walk_files(dir)? {
            let s = rel_string(&rel);
            if s.to_ascii_lowercase().ends_with(".jar") {
                let abs = dir.join(&rel);
                let bytes = std::fs::read(&abs)?;
                let mut e = PackEntry::new(&format!("mods/{s}"), EntryKind::Mod);
                e.sha1 = Some(sha1_hex(&bytes));
                e.sha512 = Some(sha512_hex(&bytes));
                e.size = Some(bytes.len() as u64);
                e.local = Some(abs);
                pack.entries.push(e);
            }
        }
        pack.entries.sort_by(|a, b| a.path.cmp(&b.path));
    }
    apply_detection(&mut pack, opts);
    Ok(pack)
}

/// Fills `mc` / `loader` from the JAR descriptors (majority wins) where the pack does not say,
/// then applies the user's `--from-*` overrides.
fn apply_detection(pack: &mut Pack, opts: &ImportOptions) {
    if pack.loader.is_none() && opts.from_loader.is_none()
        || pack.mc.is_none() && opts.from_mc.is_none()
    {
        let fs = LocalFileSystem;
        let mut loaders: Vec<(Loader, usize)> = Vec::new();
        let mut versions: BTreeMap<String, usize> = BTreeMap::new();
        for e in &pack.entries {
            let Some(jar) = &e.local else { continue };
            let Ok(a) = inspect_jar(jar, &fs) else {
                continue;
            };
            match loaders.iter_mut().find(|(l, _)| *l == a.detected_loader) {
                Some((_, n)) => *n += 1,
                None => loaders.push((a.detected_loader.clone(), 1)),
            }
            *versions.entry(fmt_mc(&a.detected_minecraft)).or_default() += 1;
        }
        // Highest count wins; ties go to the first in this fixed order.
        let rank = |l: &Loader| match l {
            Loader::Fabric => 0,
            Loader::NeoForge => 1,
            Loader::Forge => 2,
            Loader::Quilt => 3,
            _ => 4,
        };
        loaders.sort_by(|a, b| b.1.cmp(&a.1).then(rank(&a.0).cmp(&rank(&b.0))));
        if pack.loader.is_none() {
            pack.loader = loaders.first().map(|(l, _)| l.clone());
        }
        if pack.mc.is_none() {
            pack.mc = versions
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map(|(v, _)| v.clone());
        }
    }
    if let Some(l) = &opts.from_loader {
        pack.loader = Some(l.clone());
    }
    if let Some(v) = &opts.from_mc {
        pack.mc = Some(v.clone());
    }
}

/// Where a pack's `root`-relative path lands on disk.
#[must_use]
pub fn entry_path(root: &Path, rel: &str) -> PathBuf {
    root.join(rel)
}

// ---------------------------------------------------------------------------
// Writers
// ---------------------------------------------------------------------------

use super::plan::{loader_slug, PackAction, PackPlan};
use std::io::Write as _;

/// Where an exported entry's bytes come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutSource {
    /// Listed by URL and hash; the launcher downloads it. UMMP never does.
    Remote {
        url: Option<String>,
        sha1: Option<String>,
        sha512: Option<String>,
        size: Option<u64>,
        /// CurseForge ids, when the entry came from a CurseForge pack.
        curseforge: Option<(u64, u64)>,
    },
    /// A file on disk that is carried into the new pack.
    Local(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutEntry {
    pub path: String,
    pub source: OutSource,
    pub env: Env,
}

/// The pack to write: planned entries plus the target versions.
#[derive(Debug, Clone)]
pub struct OutPack {
    pub name: String,
    pub version: String,
    pub mc: String,
    pub loader: Loader,
    pub loader_version: String,
    pub entries: Vec<OutEntry>,
}

fn remote_of(e: &PackEntry) -> OutSource {
    OutSource::Remote {
        url: e.downloads.first().cloned(),
        sha1: e.sha1.clone(),
        sha512: e.sha512.clone(),
        size: e.size,
        curseforge: e.curseforge,
    }
}

fn out_source(e: &PackEntry) -> OutSource {
    e.local
        .as_ref()
        .map_or_else(|| remote_of(e), |p| OutSource::Local(p.clone()))
}

/// What goes into the new pack.
///
/// Kept entries stay as they are, updated and resolved replacements go in by URL and hash,
/// other files are unchanged. Removed, manual, ported (source only) and
/// unresolved-replacement mods are left out; the report lists them.
///
/// # Errors
/// A remote version whose file name is not a safe single path segment, or two entries that
/// land on the same path.
pub fn build_output(
    pack: &Pack,
    plan: &PackPlan,
    loader_version: &str,
) -> Result<OutPack, PackError> {
    let mut entries = Vec::new();
    for e in &pack.entries {
        if e.kind == EntryKind::Other {
            entries.push(OutEntry {
                path: e.path.clone(),
                source: out_source(e),
                env: e.env,
            });
            continue;
        }
        let Some(item) = plan.items.iter().find(|i| i.path == e.path) else {
            continue;
        };
        match &item.action {
            PackAction::Keep => entries.push(OutEntry {
                path: e.path.clone(),
                source: out_source(e),
                env: e.env,
            }),
            PackAction::Update(rv)
            | PackAction::Replace {
                resolved: Some(rv), ..
            } => {
                let name = safe_rel(&rv.file_name)?;
                if name.components().count() != 1 {
                    return Err(PackError::UnsafePath(rv.file_name.clone()));
                }
                entries.push(OutEntry {
                    path: format!("mods/{}", rv.file_name),
                    source: OutSource::Remote {
                        url: Some(rv.url.clone()),
                        sha1: Some(rv.sha1.clone()),
                        sha512: Some(rv.sha512.clone()),
                        size: Some(rv.size),
                        curseforge: None,
                    },
                    env: e.env,
                });
            }
            PackAction::Port { .. }
            | PackAction::Replace { .. }
            | PackAction::Remove { .. }
            | PackAction::Manual { .. } => {}
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    for w in entries.windows(2) {
        if w[0].path == w[1].path {
            return Err(PackError::Invalid(format!(
                "two entries land on {}",
                w[0].path
            )));
        }
    }
    Ok(OutPack {
        name: pack.name.clone(),
        version: pack.version.clone(),
        mc: plan.target.mc.clone(),
        loader: plan.target.loader.clone(),
        loader_version: loader_version.to_string(),
        entries,
    })
}

const fn support_str(s: Support) -> &'static str {
    match s {
        Support::Required => "required",
        Support::Optional => "optional",
        Support::Unsupported => "unsupported",
    }
}

/// Copies local entries into `instance` (the overrides are copied by the caller).
/// Returns warnings for entries that exist only as URLs.
///
/// # Errors
/// I/O or an unsafe entry path.
pub fn write_folder(out: &OutPack, instance: &Path) -> Result<Vec<String>, PackError> {
    let mut warnings = Vec::new();
    std::fs::create_dir_all(instance)?;
    for e in &out.entries {
        match &e.source {
            OutSource::Local(src) => {
                let dest = join_safe(instance, &e.path)?;
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(src, dest)?;
            }
            OutSource::Remote { url, .. } => warnings.push(format!(
                "{} is not in the instance folder: it is listed by URL ({}) and is not downloaded",
                e.path,
                url.as_deref().unwrap_or("no URL")
            )),
        }
    }
    Ok(warnings)
}

fn add_dir_to_zip(
    z: &mut zip::ZipWriter<File>,
    root: &Path,
    prefix: &str,
) -> Result<(), PackError> {
    let options = zip::write::FileOptions::default();
    for rel in walk_files(root)? {
        z.start_file(format!("{prefix}{}", rel_string(&rel)), options)?;
        std::io::copy(&mut File::open(root.join(&rel))?, z)?;
    }
    Ok(())
}

/// Writes a Modrinth `.mrpack`: remote entries in the index, everything in `instance`
/// (local JARs, configs, scripts) under `overrides/`.
///
/// # Errors
/// I/O or zip errors.
pub fn write_mrpack(out: &OutPack, instance: &Path, file: &Path) -> Result<(), PackError> {
    let files: Vec<serde_json::Value> = out
        .entries
        .iter()
        .filter_map(|e| match &e.source {
            OutSource::Remote {
                url: Some(url),
                sha1: Some(sha1),
                sha512: Some(sha512),
                size,
                ..
            } => Some(serde_json::json!({
                "path": e.path,
                "hashes": { "sha1": sha1, "sha512": sha512 },
                "env": { "client": support_str(e.env.client), "server": support_str(e.env.server) },
                "downloads": [url],
                "fileSize": size.unwrap_or(0),
            })),
            _ => None,
        })
        .collect();
    let key = match out.loader {
        Loader::Fabric => "fabric-loader",
        Loader::Quilt => "quilt-loader",
        Loader::NeoForge => "neoforge",
        _ => "forge",
    };
    let index = serde_json::json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": if out.version.is_empty() { "1.0.0" } else { out.version.as_str() },
        "name": out.name,
        "files": files,
        "dependencies": { "minecraft": out.mc, key: out.loader_version },
    });
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut z = zip::ZipWriter::new(File::create(file)?);
    z.start_file("modrinth.index.json", zip::write::FileOptions::default())?;
    z.write_all(
        serde_json::to_string_pretty(&index)
            .map_err(|e| PackError::Invalid(e.to_string()))?
            .as_bytes(),
    )?;
    add_dir_to_zip(&mut z, instance, "overrides/")?;
    z.finish()?;
    Ok(())
}

/// Writes a CurseForge zip. Only entries that came with CurseForge ids can be listed;
/// the rest are returned as warnings (there is no way to express a bare URL).
///
/// # Errors
/// I/O or zip errors.
pub fn write_curseforge(
    out: &OutPack,
    instance: &Path,
    file: &Path,
) -> Result<Vec<String>, PackError> {
    let mut warnings = Vec::new();
    let mut files = Vec::new();
    for e in &out.entries {
        match &e.source {
            OutSource::Remote {
                curseforge: Some((p, f)),
                ..
            } => files.push(serde_json::json!({
                "projectID": p, "fileID": f, "required": e.env.client != Support::Optional,
            })),
            OutSource::Remote { .. } => warnings.push(format!(
                "{} cannot be written to a CurseForge pack: no CurseForge ids",
                e.path
            )),
            OutSource::Local(_) => {}
        }
    }
    let manifest = serde_json::json!({
        "minecraft": {
            "version": out.mc,
            "modLoaders": [{ "id": format!("{}-{}", loader_slug(&out.loader), out.loader_version), "primary": true }],
        },
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": out.name,
        "version": out.version,
        "author": "",
        "files": files,
        "overrides": "overrides",
    });
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut z = zip::ZipWriter::new(File::create(file)?);
    z.start_file("manifest.json", zip::write::FileOptions::default())?;
    z.write_all(
        serde_json::to_string_pretty(&manifest)
            .map_err(|e| PackError::Invalid(e.to_string()))?
            .as_bytes(),
    )?;
    add_dir_to_zip(&mut z, instance, "overrides/")?;
    z.finish()?;
    Ok(warnings)
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), PackError> {
    for rel in walk_files(from)? {
        let dest = to.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from.join(&rel), dest)?;
    }
    Ok(())
}

/// Writes a Prism/MultiMC instance folder: `instance.cfg`, `mmc-pack.json` and `minecraft/`.
/// Remote-only entries cannot live in an instance and come back as warnings.
///
/// # Errors
/// I/O errors.
pub fn write_prism(out: &OutPack, instance: &Path, dir: &Path) -> Result<Vec<String>, PackError> {
    let loader_uid = match out.loader {
        Loader::Fabric => "net.fabricmc.fabric-loader",
        Loader::Quilt => "org.quiltmc.quilt-loader",
        Loader::NeoForge => "net.neoforged",
        _ => "net.minecraftforge",
    };
    let mmc = serde_json::json!({
        "formatVersion": 1,
        "components": [
            { "uid": "net.minecraft", "version": out.mc, "important": true },
            { "uid": loader_uid, "version": out.loader_version },
        ],
    });
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        dir.join("mmc-pack.json"),
        serde_json::to_string_pretty(&mmc).map_err(|e| PackError::Invalid(e.to_string()))?,
    )?;
    std::fs::write(
        dir.join("instance.cfg"),
        format!(
            "[General]\nConfigVersion=1.2\nInstanceType=OneSix\nname={}\n",
            out.name
        ),
    )?;
    copy_tree(instance, &dir.join("minecraft"))?;
    Ok(out
        .entries
        .iter()
        .filter(|e| matches!(e.source, OutSource::Remote { .. }))
        .map(|e| {
            format!(
                "{} cannot be put in a Prism instance: it is listed by URL and is not downloaded",
                e.path
            )
        })
        .collect())
}
