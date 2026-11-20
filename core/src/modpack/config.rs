//! Data rules for pack migration (`pack_config_rules.toml`): mod replacements and, from 10.E,
//! config-file migrations.

use super::{safe_rel, Pack, PackError};
use crate::checkpoint::CheckpointStore;
use crate::migration::{
    compare_versions, EditSet, FileEdit, MigrationEngine, Replacement, RunOptions,
};
use crate::project::Project;
use crate::resources::FileMove;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const RULES: &str = include_str!("../pack_config_rules.toml");

/// "Mod `from` is replaced by `to`" (data, with synthetic sample names).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReplacementRule {
    pub from: String,
    pub to: String,
    /// Target loader this applies to (`fabric`, `forge`, …); any loader when absent.
    #[serde(default)]
    pub loader: Option<String>,
    #[serde(default)]
    pub note: String,
}

/// A key renamed in a mod's config file by one Minecraft version.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct KeyRename {
    pub from: String,
    pub to: String,
}

/// "Since Minecraft `since_mc`, mod `mod_id` calls things differently in `file`."
/// Applies when the pack moves from before `since_mc` to `since_mc` or later.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ConfigRule {
    pub mod_id: String,
    pub since_mc: String,
    /// Config path relative to the instance root (`config/x.toml`).
    pub file: String,
    #[serde(default)]
    pub rename_file: Option<String>,
    #[serde(default)]
    pub rename_key: Vec<KeyRename>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PackRules {
    #[serde(default)]
    pub replacement: Vec<ReplacementRule>,
    #[serde(default)]
    pub config_rule: Vec<ConfigRule>,
}

impl PackRules {
    /// The rules shipped with UMMP.
    ///
    /// # Errors
    /// [`PackError::Invalid`] if the embedded file is malformed (a bug, covered by a test).
    pub fn builtin() -> Result<Self, PackError> {
        Self::from_toml(RULES)
    }

    /// # Errors
    /// [`PackError::Invalid`] on malformed TOML.
    pub fn from_toml(text: &str) -> Result<Self, PackError> {
        toml::from_str(text).map_err(|e| PackError::Invalid(format!("pack rules: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Config migration
// ---------------------------------------------------------------------------

/// Copies the pack's override files (configs, scripts, …) into `out`, byte for byte.
///
/// # Errors
/// I/O errors, or an override path that would leave `out`.
pub fn copy_overrides(pack: &Pack, out: &Path) -> Result<usize, PackError> {
    std::fs::create_dir_all(out)?;
    for rel in &pack.overrides {
        let dest = crate::filesystem::resolve_within(out, rel)
            .map_err(|_| PackError::UnsafePath(rel.display().to_string()))?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(pack.root.join(rel), dest)?;
    }
    Ok(pack.overrides.len())
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigChange {
    pub path: PathBuf,
    pub replacements: Vec<Replacement>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ConfigReport {
    pub changes: Vec<ConfigChange>,
    pub moves: Vec<FileMove>,
    pub warnings: Vec<String>,
    /// The checkpoint taken before applying; restoring it undoes the migration.
    pub checkpoint: Option<Uuid>,
}

/// Renames `key` at the start of a line, keeping everything else. `after` says what may
/// follow the key (so `size_x` is not `size`).
fn rename_line_key(line: &str, key: &str, to: &str, kind: &str) -> Option<String> {
    let body = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - body.len()];
    match kind {
        "toml" => {
            let rest = body.strip_prefix(key)?;
            // `size_x = 1` is a different key: only whitespace may separate key and `=`.
            rest.trim_start_matches([' ', '\t'])
                .starts_with('=')
                .then(|| format!("{indent}{to}{rest}"))
        }
        "properties" => {
            if body.starts_with('#') || body.starts_with('!') {
                return None;
            }
            let rest = body.strip_prefix(key)?;
            matches!(rest.chars().next(), Some('=' | ':' | ' ' | '\t'))
                .then(|| format!("{indent}{to}{rest}"))
        }
        "cfg" => {
            // Forge-style `I:key=value` or `S:"key"=value`.
            let mut it = body.chars();
            let ty = it.next()?;
            if !ty.is_ascii_uppercase() || it.next()? != ':' {
                return None;
            }
            let name = &body[2..];
            for (open, close) in [("", ""), ("\"", "\"")] {
                let probe = format!("{open}{key}{close}");
                if let Some(rest) = name.strip_prefix(&probe) {
                    if rest.trim_start_matches([' ', '\t']).starts_with('=') {
                        return Some(format!("{indent}{ty}:{open}{to}{close}{rest}"));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Renames a JSON object key wherever it appears as a key (at any depth), keeping formatting.
fn rename_json_key(text: &str, key: &str, to: &str, reps: &mut Vec<Replacement>) -> String {
    let needle = format!("\"{key}\"");
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut from = 0;
    while let Some(i) = text[from..].find(&needle).map(|i| i + from) {
        let end = i + needle.len();
        let next = text[end..].trim_start().chars().next();
        let prev = text[..i].trim_end().chars().last();
        if next == Some(':') && matches!(prev, Some('{' | ',') | None) {
            out.push_str(&text[last..i]);
            out.push_str("\"");
            out.push_str(to);
            out.push_str("\"");
            last = end;
            reps.push(Replacement {
                line: text[..i].matches('\n').count() + 1,
                from: key.to_string(),
                to: to.to_string(),
            });
        }
        from = end;
    }
    out.push_str(&text[last..]);
    out
}

fn rename_key(text: &str, kind: &str, k: &KeyRename, reps: &mut Vec<Replacement>) -> String {
    if kind == "json" {
        return rename_json_key(text, &k.from, &k.to, reps);
    }
    let mut out = String::with_capacity(text.len());
    for (n, line) in text.split_inclusive('\n').enumerate() {
        match rename_line_key(line, &k.from, &k.to, kind) {
            Some(new) => {
                reps.push(Replacement {
                    line: n + 1,
                    from: k.from.clone(),
                    to: k.to.clone(),
                });
                out.push_str(&new);
            }
            None => out.push_str(line),
        }
    }
    out
}

fn applies(rule: &ConfigRule, active: &BTreeSet<String>, source_mc: &str, target_mc: &str) -> bool {
    active.contains(&rule.mod_id)
        && compare_versions(source_mc, &rule.since_mc) == Some(Ordering::Less)
        && matches!(
            compare_versions(target_mc, &rule.since_mc),
            Some(Ordering::Greater | Ordering::Equal)
        )
}

/// Applies the config rules that match `active` mods and the version step to the files under
/// `out`, as an [`EditSet`] through [`MigrationEngine`] (checkpoint first, rollback on error).
///
/// # Errors
/// An unsafe rule path, I/O, or a failed apply (the checkpoint is restored first).
pub fn migrate_config(
    out: &Path,
    rules: &[ConfigRule],
    active: &BTreeSet<String>,
    source_mc: &str,
    target_mc: &str,
    snapshot_root: &Path,
) -> Result<ConfigReport, PackError> {
    let mut report = ConfigReport::default();
    // path -> (original, current, replacements); several rules may touch one file.
    let mut files: BTreeMap<PathBuf, (String, String, Vec<Replacement>)> = BTreeMap::new();
    let mut set = EditSet::default();
    for rule in rules
        .iter()
        .filter(|r| applies(r, active, source_mc, target_mc))
    {
        let rel = safe_rel(&rule.file)?;
        let abs = crate::filesystem::resolve_within(out, &rel)
            .map_err(|_| PackError::UnsafePath(rule.file.clone()))?;
        if !abs.is_file() {
            continue;
        }
        let kind = rel
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if !rule.rename_key.is_empty()
            && !matches!(kind.as_str(), "toml" | "json" | "properties" | "cfg")
        {
            report.warnings.push(format!(
                "{}: key renames are not supported for .{kind} files",
                rule.file
            ));
        } else if !rule.rename_key.is_empty() {
            let text = std::fs::read_to_string(&abs)?;
            let entry = files
                .entry(rel.clone())
                .or_insert_with(|| (text.clone(), text, Vec::new()));
            for k in &rule.rename_key {
                let mut reps = Vec::new();
                entry.1 = rename_key(&entry.1, &kind, k, &mut reps);
                entry.2.extend(reps);
            }
        }
        if let Some(to) = &rule.rename_file {
            let to = safe_rel(to)?;
            set.moves.push(FileMove {
                from: rel.clone(),
                to,
            });
        }
    }
    for (path, (original, updated, replacements)) in files {
        if original != updated {
            report.changes.push(ConfigChange {
                path: path.clone(),
                replacements: replacements.clone(),
            });
            set.edits.push(FileEdit {
                path,
                original,
                updated,
                replacements,
            });
        }
    }
    report.moves.clone_from(&set.moves);
    if set.edits.is_empty() && set.moves.is_empty() {
        return Ok(report);
    }
    let project = Project::new(out.canonicalize()?);
    let id = MigrationEngine::default().apply_edit_set(
        &set,
        &project,
        &mut CheckpointStore::new(),
        &RunOptions {
            dry_run: false,
            snapshot_root,
            session_id: Uuid::new_v4(),
        },
    )?;
    report.checkpoint = id;
    Ok(report)
}
