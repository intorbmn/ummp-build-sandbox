//! The pack pipeline end to end: import, analyse, plan, port, export, report.
//! `ummp pack migrate` is a thin wrapper around [`migrate_pack`].

use super::analyze::{analyze_pack, gate, AnalyzeOptions};
use super::config::{copy_overrides, migrate_config, PackRules};
use super::formats::{
    build_output, write_curseforge, write_folder, write_mrpack, write_prism, OutPack,
};
use super::graph::PackGraph;
use super::plan::{
    loader_slug, plan, ModIndex, PackAction, PackPlan, PlanInput, RemoteVersion, Side, Target,
};
use super::report::PackReport;
use super::{import_with, ImportOptions, Pack, PackError, PackFormat};
use crate::knowledge::KnowledgeBase;
use crate::metadata::Loader;
use crate::port::{port, PortOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where `ummp pack` writes by default.
#[must_use]
pub fn default_pack_root() -> PathBuf {
    if cfg!(windows) {
        return PathBuf::from(r"D:\Claude\ummp-packs");
    }
    std::env::var_os("HOME")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join("ummp-packs")
}

/// # Errors
/// [`PackError::Invalid`] for a loader UMMP does not know.
pub fn parse_loader(s: &str) -> Result<Loader, PackError> {
    match s.to_ascii_lowercase().as_str() {
        "forge" => Ok(Loader::Forge),
        "fabric" => Ok(Loader::Fabric),
        "neoforge" => Ok(Loader::NeoForge),
        "quilt" => Ok(Loader::Quilt),
        other => Err(PackError::Invalid(format!("unknown loader {other:?}"))),
    }
}

/// `--loader-version`, else the source pack's when the loader is unchanged, else the version
/// of the shipped build template for the target.
///
/// # Errors
/// [`PackError::Invalid`] when none of those gives a version.
pub fn resolve_loader_version(
    pack: &Pack,
    target: &Target,
    arg: Option<&str>,
) -> Result<String, PackError> {
    if let Some(v) = arg {
        return Ok(v.to_string());
    }
    if pack.loader.as_ref() == Some(&target.loader) {
        if let Some(v) = &pack.loader_version {
            return Ok(v.clone());
        }
    }
    let family = loader_slug(&target.loader);
    if let Some(t) = crate::adapter::loader::template(family, &target.mc) {
        let key = if family == "neoforge" {
            "neo_version"
        } else {
            "loader_version"
        };
        let found = t.gradle_properties.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key).then(|| v.trim().to_string())
        });
        if let Some(v) = found {
            return Ok(v);
        }
    }
    Err(PackError::Invalid(format!(
        "no {family} loader version known for Minecraft {}; pass --loader-version",
        target.mc
    )))
}

/// Remembers index answers in a JSON file so a second run does not repeat the requests.
pub struct CachedIndex<'a> {
    inner: &'a dyn ModIndex,
    path: PathBuf,
    state: Mutex<Cache>,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Cache {
    versions: BTreeMap<String, Vec<RemoteVersion>>,
    hashes: BTreeMap<String, Option<RemoteVersion>>,
}

impl<'a> CachedIndex<'a> {
    /// A cache file that is missing or unreadable starts empty.
    #[must_use]
    pub fn new(inner: &'a dyn ModIndex, path: PathBuf) -> Self {
        let state = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self {
            inner,
            path,
            state: Mutex::new(state),
        }
    }

    /// # Errors
    /// I/O errors.
    pub fn save(&self) -> Result<(), PackError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = {
            let s = self
                .state
                .lock()
                .map_err(|_| PackError::Index("cache lock".into()))?;
            serde_json::to_string_pretty(&*s).map_err(|e| PackError::Invalid(e.to_string()))?
        };
        std::fs::write(&self.path, text)?;
        Ok(())
    }
}

impl ModIndex for CachedIndex<'_> {
    fn versions(
        &self,
        project: &str,
        mc: &str,
        loader: &Loader,
    ) -> Result<Vec<RemoteVersion>, PackError> {
        let key = format!("{project}|{mc}|{}", loader_slug(loader));
        if let Some(v) = self
            .state
            .lock()
            .map_err(|_| PackError::Index("cache lock".into()))?
            .versions
            .get(&key)
        {
            return Ok(v.clone());
        }
        let v = self.inner.versions(project, mc, loader)?;
        self.state
            .lock()
            .map_err(|_| PackError::Index("cache lock".into()))?
            .versions
            .insert(key, v.clone());
        Ok(v)
    }

    fn by_hash(&self, sha1: &str) -> Result<Option<RemoteVersion>, PackError> {
        if let Some(v) = self
            .state
            .lock()
            .map_err(|_| PackError::Index("cache lock".into()))?
            .hashes
            .get(sha1)
        {
            return Ok(v.clone());
        }
        let v = self.inner.by_hash(sha1)?;
        self.state
            .lock()
            .map_err(|_| PackError::Index("cache lock".into()))?
            .hashes
            .insert(sha1.to_string(), v.clone());
        Ok(v)
    }
}

pub struct MigrateOptions {
    pub input: PathBuf,
    pub to: String,
    pub loader: String,
    pub from_loader: Option<String>,
    pub from_mc: Option<String>,
    pub online: bool,
    pub sources: Option<PathBuf>,
    pub side: Side,
    pub output: Option<PathBuf>,
    pub allow_risk: bool,
    pub loader_version: Option<String>,
    /// Which pack formats to write; empty means mrpack + folder.
    pub formats: Vec<PackFormat>,
    pub threads: usize,
    pub min_confidence: f64,
}

#[derive(Debug)]
pub struct MigrateOutcome {
    pub output: PathBuf,
    pub plan: PackPlan,
    pub report: PackReport,
    /// Files written, for the summary.
    pub written: Vec<PathBuf>,
}

impl MigrateOutcome {
    /// 0 done, 2 refused for risk, 3 done with mods left to the user.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        if self.plan.refused.is_some() {
            2
        } else if self.plan.manual_count() > 0 {
            3
        } else {
            0
        }
    }
}

fn file_safe(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if t.is_empty() {
        "pack".into()
    } else {
        t
    }
}

/// Runs the whole pipeline. A pack refused for risk writes only the report.
///
/// # Errors
/// Any import, analysis, export or migration error.
pub fn migrate_pack(
    opts: &MigrateOptions,
    index: &dyn ModIndex,
    kb: &KnowledgeBase,
) -> Result<MigrateOutcome, PackError> {
    let target_loader = parse_loader(&opts.loader)?;
    let work = tempfile::tempdir()?;
    let mut pack = import_with(
        &opts.input,
        work.path(),
        &ImportOptions {
            from_mc: opts.from_mc.clone(),
            from_loader: opts.from_loader.as_deref().map(parse_loader).transpose()?,
            ..ImportOptions::default()
        },
    )?;
    analyze_pack(
        &mut pack,
        &AnalyzeOptions {
            threads: opts.threads,
        },
    )?;
    let graph = PackGraph::build(&pack.entries);

    let out = opts.output.clone().unwrap_or_else(|| {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        default_pack_root().join(format!(
            "{}-{}-{}-{stamp}",
            file_safe(&pack.name),
            opts.to,
            file_safe(&opts.loader)
        ))
    });
    if out.exists() && std::fs::read_dir(&out)?.next().is_some() {
        return Err(PackError::Invalid(format!(
            "output folder {} is not empty",
            out.display()
        )));
    }
    std::fs::create_dir_all(&out)?;

    let target = Target {
        mc: opts.to.clone(),
        loader: target_loader,
        loader_version: opts.loader_version.clone(),
    };
    let rules = PackRules::builtin()?;
    let cached = CachedIndex::new(index, out.join(".ummp-cache/remote.json"));
    let pl = plan(
        &pack,
        &PlanInput {
            target: target.clone(),
            online: opts.online,
            sources: opts.sources.as_deref(),
            side: opts.side,
            allow_risk: opts.allow_risk,
            replacements: &rules.replacement,
        },
        &cached,
    )?;
    if opts.online {
        cached.save()?;
    }

    let mut warnings = pack.warnings.clone();
    let mut written = Vec::new();
    if pl.refused.is_none() {
        gate(&pack, opts.allow_risk)?;
        let loader_version =
            resolve_loader_version(&pack, &target, opts.loader_version.as_deref())?;
        let output = build_output(&pack, &pl, &loader_version)?;
        export(
            &pack,
            &pl,
            &output,
            &rules,
            opts,
            &out,
            &mut warnings,
            &mut written,
        )?;
        port_sources(&pl, &out, opts, kb, &mut warnings, &mut written);
    }
    let report = PackReport::build(&pack, &pl, &graph.issues, &warnings);
    let report_path = out.join("UMMP_PACK_REPORT.md");
    std::fs::write(&report_path, report.to_markdown())?;
    written.push(report_path);
    Ok(MigrateOutcome {
        output: out,
        plan: pl,
        report,
        written,
    })
}

#[allow(clippy::too_many_arguments)]
fn export(
    pack: &Pack,
    pl: &PackPlan,
    output: &OutPack,
    rules: &PackRules,
    opts: &MigrateOptions,
    out: &Path,
    warnings: &mut Vec<String>,
    written: &mut Vec<PathBuf>,
) -> Result<(), PackError> {
    let instance = out.join("instance");
    warnings.extend(write_folder(output, &instance)?);
    copy_overrides(pack, &instance)?;
    let active: BTreeSet<String> = pl
        .items
        .iter()
        .filter(|i| {
            !matches!(
                i.action,
                PackAction::Remove { .. } | PackAction::Manual { .. } | PackAction::Port { .. }
            )
        })
        .map(|i| i.mod_id.clone())
        .collect();
    if let Some(src_mc) = opts.from_mc.as_deref().or(pack.mc.as_deref()) {
        let r = migrate_config(
            &instance,
            &rules.config_rule,
            &active,
            src_mc,
            &opts.to,
            &out.join(".ummp-snapshots"),
        )?;
        warnings.extend(r.warnings);
    } else {
        warnings.push("source Minecraft version unknown: config migration skipped".into());
    }
    let formats = if opts.formats.is_empty() {
        vec![PackFormat::Mrpack, PackFormat::Folder]
    } else {
        opts.formats.clone()
    };
    let base = file_safe(&pack.name);
    for f in formats {
        match f {
            PackFormat::Folder => written.push(instance.clone()),
            PackFormat::Mrpack => {
                let p = out.join(format!("{base}.mrpack"));
                write_mrpack(output, &instance, &p)?;
                written.push(p);
            }
            PackFormat::CurseForge => {
                let p = out.join(format!("{base}-curseforge.zip"));
                warnings.extend(write_curseforge(output, &instance, &p)?);
                written.push(p);
            }
            PackFormat::Prism => {
                let p = out.join("prism").join(&base);
                warnings.extend(write_prism(output, &instance, &p)?);
                written.push(p);
            }
        }
    }
    Ok(())
}

fn port_sources(
    pl: &PackPlan,
    out: &Path,
    opts: &MigrateOptions,
    kb: &KnowledgeBase,
    warnings: &mut Vec<String>,
    written: &mut Vec<PathBuf>,
) {
    for item in &pl.items {
        let PackAction::Port { source } = &item.action else {
            continue;
        };
        let dest = out.join("ported").join(file_safe(&item.mod_id));
        let r = port(
            &PortOptions {
                input: source.clone(),
                output: Some(dest.clone()),
                to: opts.to.clone(),
                loader: Some(opts.loader.clone()),
                from: opts.from_mc.clone(),
                zip: false,
                min_confidence: opts.min_confidence,
            },
            kb,
        );
        match r {
            Ok(_) => written.push(dest),
            Err(e) => warnings.push(format!("porting {} failed: {e}", item.mod_id)),
        }
    }
}
