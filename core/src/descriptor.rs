//! Application: mod descriptor migration (Phase 6.B).
//!
//! Rewrites the Minecraft and loader version requirements in
//! `fabric.mod.json`, `quilt.mod.json`, `META-INF/mods.toml` and
//! `META-INF/neoforge.mods.toml`. Only the value strings change: formatting,
//! comments and key order are preserved so the diff stays reviewable.
//! The edited file is re-parsed; if it no longer parses, nothing is changed.

use crate::migration::{compare_versions, Replacement};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::ops::Range;
use std::path::Path;

/// What the descriptors should declare after migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescriptorTarget {
    pub minecraft: String,
    pub loader: String,
    /// Loader version for `minecraft`, from the KB compatibility table.
    pub loader_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorKind {
    Fabric,
    Quilt,
    /// `META-INF/mods.toml`: Forge, and NeoForge before 1.20.5.
    ModsToml,
    /// `META-INF/neoforge.mods.toml`: NeoForge 1.20.5+.
    NeoForgeModsToml,
}

impl DescriptorKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        let in_meta_inf = path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|p| p == "META-INF");
        match name {
            "fabric.mod.json" => Some(Self::Fabric),
            "quilt.mod.json" => Some(Self::Quilt),
            "mods.toml" if in_meta_inf => Some(Self::ModsToml),
            "neoforge.mods.toml" if in_meta_inf => Some(Self::NeoForgeModsToml),
            _ => None,
        }
    }

    fn accepts_loader(self, loader: &str) -> bool {
        match self {
            Self::Fabric => loader == "fabric",
            Self::Quilt => loader == "quilt",
            Self::ModsToml => loader == "forge" || loader == "neoforge",
            Self::NeoForgeModsToml => loader == "neoforge",
        }
    }
}

/// Result of a text transform: new content, what changed, and notes.
#[derive(Debug, Clone, Default)]
pub struct EditResult {
    pub updated: String,
    pub replacements: Vec<Replacement>,
    pub warnings: Vec<String>,
}

/// `1.17.1` -> `1.18`; `None` for non-numeric versions.
fn next_minor(v: &str) -> Option<String> {
    let mut parts = v.split('.').map(|p| p.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    Some(format!("{major}.{}", minor + 1))
}

/// New Minecraft requirement in the same style as `old`.
/// Maven ranges (`[1.20.1,1.21)`) become `[target,next-minor)`; operator
/// forms (`>=1.20.1`, `~1.20.1`, `1.20.1`) keep their operator; wildcards
/// (`1.20.x`) become `~target`; `*` is left alone.
pub(crate) fn retarget_minecraft(old: &str, target: &str) -> Option<String> {
    let old = old.trim();
    if old == "*" || old.is_empty() {
        return None;
    }
    if old.starts_with('[') || old.starts_with('(') {
        return Some(next_minor(target).map_or_else(
            || format!("[{target}]"),
            |upper| format!("[{target},{upper})"),
        ));
    }
    let start = old.find(|c: char| c.is_ascii_digit())?;
    let (prefix, rest) = old.split_at(start);
    if rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
        Some(format!("{prefix}{target}"))
    } else {
        Some(format!("~{target}"))
    }
}

/// New loader requirement for loader version `lv`, in the style of `old`.
/// MDK major-only ranges stay major-only (`[36,)` -> `[37,)`); other maven
/// ranges become `[lv,)`; operator forms keep the operator (`>=0.14` -> `>=lv`).
pub(crate) fn retarget_loader(old: &str, lv: &str) -> Option<String> {
    let old = old.trim();
    if old == "*" || old.is_empty() {
        return None;
    }
    let major = lv.split('.').next().unwrap_or(lv);
    if let Some(inner) = old.strip_prefix('[').and_then(|s| s.strip_suffix(",)")) {
        if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("[{major},)"));
        }
    }
    if old.starts_with('[') || old.starts_with('(') {
        return Some(format!("[{lv},)"));
    }
    let start = old.find(|c: char| c.is_ascii_digit())?;
    Some(format!("{}{lv}", &old[..start]))
}

/// Apply value edits back to front and validate the result.
/// Returns `None` when `valid` rejects the edited text.
pub(crate) fn apply_edits(
    content: &str,
    mut edits: Edits,
    valid: impl Fn(&str) -> bool,
) -> Option<(String, Vec<Replacement>)> {
    // Collectors may find values in any order; sort so offsets stay valid.
    edits.sort_by_key(|(range, _, _)| range.start);
    let mut updated = content.to_string();
    let mut replacements = Vec::new();
    for (range, old, new) in edits.iter().rev() {
        updated.replace_range(range.clone(), new);
        replacements.push(Replacement {
            line: 1 + content[..range.start].matches('\n').count(),
            from: old.clone(),
            to: new.clone(),
        });
    }
    replacements.reverse();
    valid(&updated).then_some((updated, replacements))
}

/// Collected value edits: byte range of the value contents, old, new.
pub(crate) type Edits = Vec<(Range<usize>, String, String)>;

pub fn migrate_descriptor(
    kind: DescriptorKind,
    content: &str,
    target: &DescriptorTarget,
) -> EditResult {
    let mut result = EditResult {
        updated: content.to_string(),
        ..EditResult::default()
    };
    if !kind.accepts_loader(&target.loader) {
        result.warnings.push(format!(
            "skipped: {kind:?} descriptor but target loader is '{}' \
             (loader-to-loader migration is not supported)",
            target.loader
        ));
        return result;
    }
    if kind == DescriptorKind::ModsToml
        && target.loader == "neoforge"
        && matches!(
            compare_versions(&target.minecraft, "1.20.5"),
            Some(Ordering::Greater | Ordering::Equal)
        )
    {
        result.warnings.push(
            "NeoForge 1.20.5+ reads META-INF/neoforge.mods.toml: rename this file manually".into(),
        );
    }

    let mut wants_loader = false;
    let edits = match kind {
        DescriptorKind::Fabric | DescriptorKind::Quilt => {
            json_edits(kind, content, target, &mut wants_loader)
        }
        DescriptorKind::ModsToml | DescriptorKind::NeoForgeModsToml => {
            toml_edits(content, target, &mut wants_loader)
        }
    };
    if wants_loader && target.loader_version.is_none() {
        result.warnings.push(format!(
            "no {} version for Minecraft {} in the knowledge base; loader requirement left unchanged",
            target.loader, target.minecraft
        ));
    }
    if edits.is_empty() {
        return result;
    }
    let valid = |t: &str| match kind {
        DescriptorKind::Fabric | DescriptorKind::Quilt => {
            serde_json::from_str::<serde_json::Value>(t).is_ok()
        }
        _ => t.parse::<toml::Table>().is_ok(),
    };
    match apply_edits(content, edits, valid) {
        Some((updated, replacements)) => {
            result.updated = updated;
            result.replacements = replacements;
        }
        None => result
            .warnings
            .push("edit would produce an unparsable descriptor; left unchanged".into()),
    }
    result
}

// ---------------------------------------------------------------------------
// JSON (fabric.mod.json, quilt.mod.json)
// ---------------------------------------------------------------------------

/// Index just past the string literal starting at `i` (a `"`).
pub(crate) fn skip_json_string(b: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => return j + 1,
            _ => j += 1,
        }
    }
    b.len()
}

/// Index of the bracket closing the one at `open`.
pub(crate) fn matching_close(b: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'"' => {
                i = skip_json_string(b, i);
                continue;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

pub(crate) fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Start index of the value of `key` among the direct members of the object
/// whose `{` is at `obj`.
pub(crate) fn find_key(text: &str, obj: usize, key: &str) -> Option<usize> {
    let b = text.as_bytes();
    let end = matching_close(b, obj)?;
    let mut i = obj + 1;
    let mut depth = 0usize;
    while i < end {
        match b[i] {
            b'"' => {
                let after = skip_json_string(b, i);
                if depth == 0 && &text[i + 1..after - 1] == key {
                    let colon = skip_ws(b, after);
                    if b.get(colon) == Some(&b':') {
                        return Some(skip_ws(b, colon + 1));
                    }
                }
                i = after;
                continue;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Contents range of the string value starting at `v` (a `"`), if it has no escapes.
pub(crate) fn string_value(text: &str, v: usize) -> Option<Range<usize>> {
    let b = text.as_bytes();
    if b.get(v) != Some(&b'"') {
        return None;
    }
    let end = skip_json_string(b, v) - 1;
    let range = v + 1..end;
    (!text[range.clone()].contains('\\')).then_some(range)
}

pub(crate) fn push_edit(edits: &mut Edits, text: &str, range: Range<usize>, new: Option<String>) {
    if let Some(new) = new {
        if text[range.clone()] != new {
            let old = text[range.clone()].to_string();
            edits.push((range, old, new));
        }
    }
}

fn json_edits(
    kind: DescriptorKind,
    text: &str,
    target: &DescriptorTarget,
    wants_loader: &mut bool,
) -> Edits {
    let mut edits = Vec::new();
    let b = text.as_bytes();
    let Some(root) = text.find('{') else {
        return edits;
    };

    if kind == DescriptorKind::Fabric {
        let Some(deps) = find_key(text, root, "depends").filter(|&d| b[d] == b'{') else {
            return edits;
        };
        if let Some(r) = find_key(text, deps, "minecraft").and_then(|v| string_value(text, v)) {
            let new = retarget_minecraft(&text[r.clone()], &target.minecraft);
            push_edit(&mut edits, text, r, new);
        }
        if let Some(r) = find_key(text, deps, "fabricloader").and_then(|v| string_value(text, v)) {
            *wants_loader = true;
            let new = target
                .loader_version
                .as_deref()
                .and_then(|lv| retarget_loader(&text[r.clone()], lv));
            push_edit(&mut edits, text, r, new);
        }
        return edits;
    }

    // Quilt: quilt_loader.depends is an array of strings or objects.
    let Some(ql) = find_key(text, root, "quilt_loader").filter(|&q| b[q] == b'{') else {
        return edits;
    };
    let Some(arr) = find_key(text, ql, "depends").filter(|&a| b[a] == b'[') else {
        return edits;
    };
    let Some(arr_end) = matching_close(b, arr) else {
        return edits;
    };
    let mut i = arr + 1;
    while i < arr_end {
        match b[i] {
            b'"' => i = skip_json_string(b, i),
            b'{' => {
                let close = matching_close(b, i).unwrap_or(arr_end);
                let id = find_key(text, i, "id")
                    .and_then(|v| string_value(text, v))
                    .map(|r| &text[r]);
                let versions = find_key(text, i, "versions").and_then(|v| string_value(text, v));
                if let Some(r) = versions {
                    match id {
                        Some("minecraft") => {
                            let new = retarget_minecraft(&text[r.clone()], &target.minecraft);
                            push_edit(&mut edits, text, r, new);
                        }
                        Some("quilt_loader") => {
                            *wants_loader = true;
                            let new = target
                                .loader_version
                                .as_deref()
                                .and_then(|lv| retarget_loader(&text[r.clone()], lv));
                            push_edit(&mut edits, text, r, new);
                        }
                        _ => {}
                    }
                }
                i = close + 1;
            }
            _ => i += 1,
        }
    }
    edits
}

// ---------------------------------------------------------------------------
// TOML (mods.toml, neoforge.mods.toml)
// ---------------------------------------------------------------------------

/// Range of the quoted value of `key = "..."` on one line (offset `base`).
pub(crate) fn toml_value(line: &str, base: usize, key: &str) -> Option<Range<usize>> {
    let t = line.trim_start();
    let rest = t.strip_prefix(key)?;
    let rest_trim = rest.trim_start();
    let after_eq = rest_trim.strip_prefix('=')?.trim_start();
    let quote = after_eq
        .chars()
        .next()
        .filter(|c| *c == '"' || *c == '\'')?;
    let start_in_line = line.len() - after_eq.len() + 1;
    let len = after_eq[1..].find(quote)?;
    Some(base + start_in_line..base + start_in_line + len)
}

fn toml_edits(text: &str, target: &DescriptorTarget, wants_loader: &mut bool) -> Edits {
    let mut edits = Vec::new();
    // (modId, versionRange range) per [[dependencies.*]] block
    let mut blocks: Vec<(Option<String>, Option<Range<usize>>)> = Vec::new();
    let mut in_dep = false;
    let mut top_level = true;
    let mut loader_version_value: Option<Range<usize>> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        if top_level && !t.starts_with('[') {
            if let Some(r) = toml_value(line, offset, "loaderVersion") {
                loader_version_value = Some(r);
            }
        }
        if t.starts_with('[') {
            top_level = false;
            in_dep = t.starts_with("[[dependencies.");
            if in_dep {
                blocks.push((None, None));
            }
        } else if in_dep {
            if let Some(block) = blocks.last_mut() {
                if let Some(r) = toml_value(line, offset, "modId") {
                    block.0 = Some(text[r].to_string());
                } else if let Some(r) = toml_value(line, offset, "versionRange") {
                    block.1 = Some(r);
                }
            }
        }
        offset += line.len();
    }

    let lv = target.loader_version.as_deref();
    for (mod_id, range) in blocks {
        let (Some(mod_id), Some(r)) = (mod_id, range) else {
            continue;
        };
        if mod_id == "minecraft" {
            let new = retarget_minecraft(&text[r.clone()], &target.minecraft);
            push_edit(&mut edits, text, r, new);
        } else if mod_id == target.loader {
            *wants_loader = true;
            // Forge's javafml loaderVersion tracks the Forge major version;
            // update it only when it equals the old Forge range.
            if let (Some(lvr), Some(lv)) = (&loader_version_value, lv) {
                if target.loader == "forge" && text[lvr.clone()] == text[r.clone()] {
                    let new = retarget_loader(&text[lvr.clone()], lv);
                    push_edit(&mut edits, text, lvr.clone(), new);
                }
            }
            let new = lv.and_then(|lv| retarget_loader(&text[r.clone()], lv));
            push_edit(&mut edits, text, r, new);
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(loader: &str, mc: &str, lv: Option<&str>) -> DescriptorTarget {
        DescriptorTarget {
            minecraft: mc.into(),
            loader: loader.into(),
            loader_version: lv.map(Into::into),
        }
    }

    #[test]
    fn test_retarget_loader_keeps_style() {
        assert_eq!(retarget_loader("[36,)", "37.1.1").unwrap(), "[37,)");
        assert_eq!(retarget_loader("[36.2.0,)", "37.1.1").unwrap(), "[37.1.1,)");
        assert_eq!(retarget_loader(">=0.14.21", "0.16.0").unwrap(), ">=0.16.0");
        assert_eq!(retarget_loader("*", "0.16.0"), None);
    }

    #[test]
    fn test_retarget_minecraft_keeps_style() {
        let t = "1.21.1";
        assert_eq!(
            retarget_minecraft("[1.20.1,1.21)", t).unwrap(),
            "[1.21.1,1.22)"
        );
        assert_eq!(retarget_minecraft(">=1.20.1", t).unwrap(), ">=1.21.1");
        assert_eq!(retarget_minecraft("~1.20.1", t).unwrap(), "~1.21.1");
        assert_eq!(retarget_minecraft("1.20.1", t).unwrap(), "1.21.1");
        assert_eq!(retarget_minecraft("1.20.x", t).unwrap(), "~1.21.1");
        assert_eq!(retarget_minecraft("*", t), None);
    }

    #[test]
    fn test_descriptor_kind_from_path() {
        use DescriptorKind as K;
        let k = |p: &str| DescriptorKind::from_path(Path::new(p));
        assert_eq!(k("src/main/resources/fabric.mod.json"), Some(K::Fabric));
        assert_eq!(
            k("src/main/resources/META-INF/mods.toml"),
            Some(K::ModsToml)
        );
        assert_eq!(
            k("src/main/resources/META-INF/neoforge.mods.toml"),
            Some(K::NeoForgeModsToml)
        );
        assert_eq!(k("config/mods.toml"), None); // not a descriptor
    }

    #[test]
    fn test_fabric_preserves_formatting() {
        let src = "{\n  \"schemaVersion\": 1,\n  \"id\": \"demo\",\n  \"depends\": {\n    \
                   \"fabricloader\": \">=0.14.21\",\n    \"minecraft\": \"~1.20.1\",\n    \
                   \"java\": \">=17\"\n  },\n  \"suggests\": { \"minecraft\": \"1.20.1\" }\n}\n";
        let r = migrate_descriptor(
            DescriptorKind::Fabric,
            src,
            &target("fabric", "1.21.1", Some("0.16.0")),
        );
        let expected = src
            .replace("\">=0.14.21\"", "\">=0.16.0\"")
            .replace("\"~1.20.1\"", "\"~1.21.1\"");
        assert_eq!(r.updated, expected); // "suggests" untouched
        assert_eq!(r.replacements.len(), 2);
        assert_eq!(r.replacements[0].line, 5);
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn test_quilt_object_and_string_dependencies() {
        let src = r#"{"schema_version":1,"quilt_loader":{"id":"demo","depends":[
  {"id":"quilt_loader","versions":">=0.19.0"},
  "qsl",
  {"id":"minecraft","versions":">=1.20.1"}
]}}"#;
        let r = migrate_descriptor(
            DescriptorKind::Quilt,
            src,
            &target("quilt", "1.21", Some("0.26.0")),
        );
        assert!(r.updated.contains(r#""versions":">=1.21""#));
        assert!(r.updated.contains(r#""versions":">=0.26.0""#));
        assert_eq!(r.replacements.len(), 2);
    }

    const FORGE_TOML: &str = r#"modLoader="javafml" # comment kept
loaderVersion="[36,)"
license="MIT"
[[mods]]
modId="demo"
version="1.0.0"
[[dependencies.demo]]
    modId="forge"
    mandatory=true
    versionRange="[36,)"
    ordering="NONE"
    side="BOTH"
[[dependencies.demo]]
    modId="minecraft"
    mandatory=true
    versionRange="[1.16.5,1.17)"
    ordering="NONE"
    side="BOTH"
"#;

    #[test]
    fn test_mods_toml_minecraft_and_loader() {
        let r = migrate_descriptor(
            DescriptorKind::ModsToml,
            FORGE_TOML,
            &target("forge", "1.17.1", Some("37.1.1")),
        );
        // MDK style: major-only ranges stay major-only, and Forge's javafml
        // loaderVersion follows the Forge major.
        let expected = FORGE_TOML
            .replace("\"[36,)\"", "\"[37,)\"")
            .replace("\"[1.16.5,1.17)\"", "\"[1.17.1,1.18)\"");
        assert_eq!(r.updated, expected);
        let lines: Vec<usize> = r.replacements.iter().map(|x| x.line).collect();
        assert_eq!(lines, [2, 10, 16]);
    }

    #[test]
    fn test_missing_loader_version_warns_and_keeps_loader_range() {
        let r = migrate_descriptor(
            DescriptorKind::ModsToml,
            FORGE_TOML,
            &target("forge", "1.17.1", None),
        );
        assert!(r.updated.contains("versionRange=\"[36,)\""));
        assert!(r.updated.contains("\"[1.17.1,1.18)\""));
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn test_loader_mismatch_is_skipped() {
        let r = migrate_descriptor(
            DescriptorKind::ModsToml,
            FORGE_TOML,
            &target("fabric", "1.17.1", None),
        );
        assert_eq!(r.updated, FORGE_TOML);
        assert!(r.replacements.is_empty());
        assert!(r.warnings[0].starts_with("skipped"));
    }

    #[test]
    fn test_neoforge_mods_toml_rename_warning() {
        let src = FORGE_TOML.replace("modId=\"forge\"", "modId=\"neoforge\"");
        let r = migrate_descriptor(
            DescriptorKind::ModsToml,
            &src,
            &target("neoforge", "1.21", Some("21.0.167")),
        );
        assert!(r.updated.contains("versionRange=\"[21,)\""));
        // NeoForge's loaderVersion is the FML version, not NeoForge's: untouched.
        assert!(r.updated.contains("loaderVersion=\"[36,)\""));
        assert!(r.warnings.iter().any(|w| w.contains("neoforge.mods.toml")));
    }
}
