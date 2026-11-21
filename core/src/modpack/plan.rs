//! Per-mod decisions for a pack migration (guide §6).
//!
//! Rules run in order and the first match
//! wins; every decision carries a reason for the report. A second pass re-checks the
//! dependencies of the planned pack until nothing changes.

use super::config::ReplacementRule;
use super::graph::BUILTIN_IDS;
use super::{fmt_mc, safe_rel, EntryKind, Env, Pack, PackEntry, PackError, Support};
use crate::metadata::Loader;
use crate::migration::compare_versions;
use crate::scan::Severity;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// A version of a mod known to an online index (Modrinth).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteVersion {
    pub project: String,
    pub version: String,
    pub file_name: String,
    pub url: String,
    pub sha1: String,
    pub sha512: String,
    pub size: u64,
    /// Project ids this version requires.
    pub required_deps: Vec<String>,
    pub env: Env,
}

/// Read-only lookups against an online mod index. Used only with `--online`.
pub trait ModIndex {
    /// Versions of `project` that support `mc` + `loader`, newest first.
    ///
    /// # Errors
    /// [`PackError::Index`] when the lookup fails.
    fn versions(
        &self,
        project: &str,
        mc: &str,
        loader: &Loader,
    ) -> Result<Vec<RemoteVersion>, PackError>;

    /// The remote version of a JAR with this sha1, if the index knows it.
    ///
    /// # Errors
    /// [`PackError::Index`] when the lookup fails.
    fn by_hash(&self, sha1: &str) -> Result<Option<RemoteVersion>, PackError>;
}

/// The name an index and the rules use for a loader.
#[must_use]
pub const fn loader_slug(l: &Loader) -> &'static str {
    match l {
        Loader::Forge => "forge",
        Loader::Fabric => "fabric",
        Loader::NeoForge => "neoforge",
        Loader::Quilt => "quilt",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Target {
    pub mc: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Client,
    Server,
    Both,
}

pub struct PlanInput<'a> {
    pub target: Target,
    /// Allow calls to the index. Offline (`false`) never touches it.
    pub online: bool,
    /// Folder holding `<mod-id>/` or `<mod-id>.zip` sources for mods the user can port.
    pub sources: Option<&'a std::path::Path>,
    pub side: Side,
    pub allow_risk: bool,
    pub replacements: &'a [ReplacementRule],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PackAction {
    Keep,
    Update(RemoteVersion),
    /// Port from the user's source with `port::port`.
    Port {
        source: PathBuf,
    },
    Replace {
        with: String,
        /// The replacement's download, known only with `--online`.
        resolved: Option<RemoteVersion>,
    },
    Remove {
        reason: String,
    },
    Manual {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanItem {
    /// The entry's path in the source pack.
    pub path: String,
    pub mod_id: String,
    pub action: PackAction,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackPlan {
    pub target: Target,
    pub items: Vec<PlanItem>,
    /// Set when a High-risk JAR stopped the migration (exit code 2).
    pub refused: Option<String>,
}

impl PackPlan {
    /// Mods the user still has to deal with (exit code 3).
    #[must_use]
    pub fn manual_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| {
                matches!(
                    i.action,
                    PackAction::Manual { .. } | PackAction::Replace { resolved: None, .. }
                )
            })
            .count()
    }
}

fn mc_equal(a: &str, b: &str) -> bool {
    compare_versions(a, b) == Some(Ordering::Equal)
}

/// A single path segment, so `sources.join(id)` cannot leave `sources`.
fn single_segment(id: &str) -> bool {
    safe_rel(id).is_ok_and(|p| p.components().count() == 1)
}

fn find_source(dir: &std::path::Path, id: &str) -> Option<PathBuf> {
    if !single_segment(id) {
        return None;
    }
    let folder = dir.join(id);
    if folder.is_dir() {
        return Some(folder);
    }
    let zip = dir.join(format!("{id}.zip"));
    zip.is_file().then_some(zip)
}

fn decide(
    e: &PackEntry,
    pack: &Pack,
    inp: &PlanInput<'_>,
    index: &dyn ModIndex,
) -> (PackAction, String) {
    let t = &inp.target;
    let id = e.display_id();
    let known = e
        .mod_info
        .as_ref()
        .filter(|m| !matches!(m.detected_loader, Loader::Unknown(_)));

    // 1. Malware risk.
    if e.risk == Some(Severity::High) && !inp.allow_risk {
        return (
            PackAction::Remove {
                reason: "malware risk".into(),
            },
            "High malware risk: not carried over".into(),
        );
    }
    // 2. Replacement rule.
    if let Some(rule) = inp.replacements.iter().find(|r| {
        r.from == id
            && r.loader
                .as_deref()
                .is_none_or(|l| l == loader_slug(&t.loader))
    }) {
        let resolved = if inp.online {
            index
                .versions(&rule.to, &t.mc, &t.loader)
                .ok()
                .and_then(|v| v.into_iter().next())
        } else {
            None
        };
        let why = if resolved.is_some() {
            format!("replaced by {} ({})", rule.to, rule.note)
        } else if inp.online {
            format!(
                "replaced by {}: no version for {}/{}",
                rule.to,
                t.mc,
                loader_slug(&t.loader)
            )
        } else {
            format!(
                "replaced by {}: install it yourself or run with --online",
                rule.to
            )
        };
        return (
            PackAction::Replace {
                with: rule.to.clone(),
                resolved,
            },
            why,
        );
    }
    // 3. Already supports the target.
    if let Some(m) = known {
        if m.detected_loader == t.loader && mc_equal(&fmt_mc(&m.detected_minecraft), &t.mc) {
            return (
                PackAction::Keep,
                format!("descriptor targets {}/{}", t.mc, loader_slug(&t.loader)),
            );
        }
    } else if e.mod_info.is_none()
        && pack.loader.as_ref() == Some(&t.loader)
        && pack.mc.as_deref().is_some_and(|v| mc_equal(v, &t.mc))
    {
        return (
            PackAction::Keep,
            "same Minecraft version and loader as the source pack".into(),
        );
    }
    // 4. Online update.
    let mut lookup_error = None;
    if inp.online {
        match find_update(e, t, index) {
            Ok(Some(rv)) => {
                let why = format!(
                    "{} {} supports {}/{}",
                    rv.project,
                    rv.version,
                    t.mc,
                    loader_slug(&t.loader)
                );
                return (PackAction::Update(rv), why);
            }
            Ok(None) => {}
            Err(err) => lookup_error = Some(err.to_string()),
        }
    }
    // 5. Source supplied.
    if let (Some(dir), true) = (inp.sources, known.is_some()) {
        if let Some(source) = find_source(dir, &id) {
            return (
                PackAction::Port {
                    source: source.clone(),
                },
                format!("source supplied at {}", source.display()),
            );
        }
    }
    // 6. Wrong side.
    let wrong = match inp.side {
        Side::Client => e.env.client == Support::Unsupported,
        Side::Server => e.env.server == Support::Unsupported,
        Side::Both => false,
    };
    if wrong {
        return (
            PackAction::Remove {
                reason: "wrong side".into(),
            },
            format!(
                "not supported on the {} side",
                if inp.side == Side::Client {
                    "client"
                } else {
                    "server"
                }
            ),
        );
    }
    // 7. Left to the user.
    let reason = if let Some((p, f)) = e.curseforge {
        format!("CurseForge file {p}/{f} is unresolved: resolving it needs a CurseForge API key")
    } else if e.mod_info.is_none() {
        "no descriptor available offline: provide the JAR or run with --online".into()
    } else if let Some(err) = lookup_error {
        format!(
            "no version for {}/{} (lookup failed: {err})",
            t.mc,
            loader_slug(&t.loader)
        )
    } else {
        format!("no version for {}/{}", t.mc, loader_slug(&t.loader))
    };
    (
        PackAction::Manual {
            reason: reason.clone(),
        },
        reason,
    )
}

fn find_update(
    e: &PackEntry,
    t: &Target,
    index: &dyn ModIndex,
) -> Result<Option<RemoteVersion>, PackError> {
    let by_hash = match &e.sha1 {
        Some(h) => index.by_hash(h)?.map(|v| v.project),
        None => None,
    };
    let Some(project) = by_hash.or_else(|| e.mod_info.as_ref().map(|m| m.metadata.mod_id.clone()))
    else {
        return Ok(None);
    };
    Ok(index
        .versions(&project, &t.mc, &t.loader)?
        .into_iter()
        .next())
}

/// Plans every mod of `pack` for `inp.target`.
///
/// # Errors
/// Currently none; index failures become `Manual` decisions with the failure in the reason.
pub fn plan(pack: &Pack, inp: &PlanInput<'_>, index: &dyn ModIndex) -> Result<PackPlan, PackError> {
    let mods: Vec<&PackEntry> = pack
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Mod)
        .collect();
    let mut items: Vec<PlanItem> = mods
        .iter()
        .map(|e| {
            let (action, reason) = decide(e, pack, inp, index);
            PlanItem {
                path: e.path.clone(),
                mod_id: e.display_id(),
                action,
                reason,
            }
        })
        .collect();
    recheck_dependencies(&mods, &mut items);

    let refused = {
        let bad: Vec<&str> = mods
            .iter()
            .filter(|e| e.risk == Some(Severity::High))
            .map(|e| e.path.as_str())
            .collect();
        (!inp.allow_risk && !bad.is_empty())
            .then(|| format!("High malware risk in {}", bad.join(", ")))
    };
    Ok(PackPlan {
        target: inp.target.clone(),
        items,
        refused,
    })
}

/// A kept, updated or ported mod whose required dependency was removed or left to the user
/// becomes `Manual`. Repeats until stable; each pass changes at least one item, so the number
/// of mods bounds the loop.
fn recheck_dependencies(mods: &[&PackEntry], items: &mut [PlanItem]) {
    let in_pack: BTreeSet<String> = mods
        .iter()
        .filter_map(|e| e.mod_info.as_ref().map(|m| m.metadata.mod_id.clone()))
        .collect();
    for _ in 0..=items.len() {
        let mut avail: BTreeSet<&str> = BTreeSet::new();
        for i in items.iter() {
            match &i.action {
                PackAction::Keep | PackAction::Update(_) | PackAction::Port { .. } => {
                    avail.insert(i.mod_id.as_str());
                }
                PackAction::Replace { with, .. } => {
                    avail.insert(i.mod_id.as_str());
                    avail.insert(with.as_str());
                }
                _ => {}
            }
        }
        let avail: BTreeSet<String> = avail.into_iter().map(str::to_string).collect();
        let mut changed = false;
        for (e, item) in mods.iter().zip(items.iter_mut()) {
            if !matches!(
                item.action,
                PackAction::Keep | PackAction::Update(_) | PackAction::Port { .. }
            ) {
                continue;
            }
            let Some(m) = &e.mod_info else { continue };
            let mut missing: Vec<String> = m
                .metadata
                .dependencies
                .iter()
                .filter(|d| !d.optional && !BUILTIN_IDS.contains(&d.mod_id.as_str()))
                .filter(|d| in_pack.contains(&d.mod_id) && !avail.contains(&d.mod_id))
                .map(|d| d.mod_id.clone())
                .collect();
            missing.sort();
            missing.dedup();
            if !missing.is_empty() {
                let reason = format!("missing {}", missing.join(", "));
                item.action = PackAction::Manual {
                    reason: reason.clone(),
                };
                item.reason = reason;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}
