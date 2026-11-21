//! Application: resource and data pack migration (Phase 6.D).
//!
//! - `pack.mcmeta`: sets `pack.pack_format` (and Forge's
//!   `forge:resource_pack_format` / `forge:data_pack_format`) to the target's
//!   formats from the knowledge base. Formats are never guessed.
//! - Data pack folders renamed to singular in Minecraft 1.21 are moved
//!   (`data/<ns>/recipes/` -> `data/<ns>/recipe/`, `tags/items` -> `tags/item`).

use crate::descriptor::{apply_edits, find_key, EditResult, Edits};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Folders renamed in Minecraft 1.21 (snapshots 24w19a/24w21a).
pub const DATAPACK_RENAMES_1_21: &[(&str, &str)] = &[
    ("advancements", "advancement"),
    ("functions", "function"),
    ("item_modifiers", "item_modifier"),
    ("loot_tables", "loot_table"),
    ("predicates", "predicate"),
    ("recipes", "recipe"),
    ("structures", "structure"),
    ("tags/blocks", "tags/block"),
    ("tags/entity_types", "tags/entity_type"),
    ("tags/fluids", "tags/fluid"),
    ("tags/functions", "tags/function"),
    ("tags/game_events", "tags/game_event"),
    ("tags/items", "tags/item"),
];

/// A file moved to a new path (relative to the project root).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMove {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// New path for a file under `data/<namespace>/<renamed folder>/`, if any.
pub fn datapack_move(rel: &Path) -> Option<PathBuf> {
    let parts: Vec<&str> = rel
        .components()
        .map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let data = parts.iter().position(|p| *p == "data")?;
    let rest = parts.get(data + 2..)?; // after data/<namespace>/
    let rest_joined = rest.join("/");
    for (old, new) in DATAPACK_RENAMES_1_21 {
        if let Some(tail) = rest_joined
            .strip_prefix(old)
            .and_then(|t| t.strip_prefix('/'))
        {
            let mut out: PathBuf = parts[..data + 2].iter().collect();
            out.push(new);
            out.push(tail);
            return Some(out);
        }
    }
    None
}

pub fn is_pack_mcmeta(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n == "pack.mcmeta")
}

/// Target pack formats for one Minecraft version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackFormats {
    pub resource: Option<u32>,
    pub data: Option<u32>,
}

/// Number literal range at `v`.
fn number_value(text: &str, v: usize) -> Option<std::ops::Range<usize>> {
    let len = text[v..]
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len() - v);
    (len > 0).then_some(v..v + len)
}

/// `data_only`: the pack has a `data/` folder but no `assets/` (a data pack);
/// otherwise `pack_format` is the resource pack format, as in mod resources.
pub fn migrate_pack_mcmeta(content: &str, formats: PackFormats, data_only: bool) -> EditResult {
    let mut result = EditResult {
        updated: content.to_string(),
        ..EditResult::default()
    };
    let b = content.as_bytes();
    let Some(pack) = content
        .find('{')
        .and_then(|root| find_key(content, root, "pack"))
        .filter(|&p| b[p] == b'{')
    else {
        return result;
    };
    let main = if data_only {
        formats.data
    } else {
        formats.resource
    };
    let mut edits: Edits = Vec::new();
    for (key, fmt) in [
        ("pack_format", main),
        ("forge:resource_pack_format", formats.resource),
        ("forge:data_pack_format", formats.data),
    ] {
        let Some(r) = find_key(content, pack, key).and_then(|v| number_value(content, v)) else {
            continue;
        };
        match fmt {
            Some(f) if content[r.clone()] != f.to_string() => {
                edits.push((r.clone(), content[r].to_string(), f.to_string()));
            }
            Some(_) => {}
            None => result.warnings.push(format!(
                "{key}: no pack format for the target version in the knowledge base"
            )),
        }
    }
    if find_key(content, pack, "supported_formats").is_some() {
        result
            .warnings
            .push("supported_formats present: review it manually".into());
    }
    let valid = |t: &str| serde_json::from_str::<serde_json::Value>(t).is_ok();
    if let Some((updated, replacements)) = apply_edits(content, edits, valid) {
        result.updated = updated;
        result.replacements = replacements;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datapack_moves() {
        let m =
            |p: &str| datapack_move(Path::new(p)).map(|p| p.to_string_lossy().replace('\\', "/"));
        assert_eq!(
            m("src/main/resources/data/demo/recipes/widget.json").unwrap(),
            "src/main/resources/data/demo/recipe/widget.json"
        );
        assert_eq!(
            m("data/demo/tags/items/sub/x.json").unwrap(),
            "data/demo/tags/item/sub/x.json"
        );
        assert_eq!(m("data/demo/recipe/widget.json"), None); // already singular
        assert_eq!(m("data/demo/worldgen/biome/x.json"), None);
        assert_eq!(m("assets/demo/recipes/x.json"), None);
    }

    #[test]
    fn test_pack_mcmeta_mod_and_forge_keys() {
        let src = "{\n  \"pack\": {\n    \"description\": \"demo\",\n    \"pack_format\": 6,\n    \
                   \"forge:resource_pack_format\": 6,\n    \"forge:data_pack_format\": 6\n  }\n}\n";
        let f = PackFormats {
            resource: Some(7),
            data: Some(8),
        };
        let r = migrate_pack_mcmeta(src, f, false);
        assert!(r.updated.contains("\"pack_format\": 7,"));
        assert!(r.updated.contains("\"forge:resource_pack_format\": 7,"));
        assert!(r.updated.contains("\"forge:data_pack_format\": 8\n"));
        let r = migrate_pack_mcmeta(src, f, true);
        assert!(r.updated.contains("\"pack_format\": 8,"));
    }

    #[test]
    fn test_pack_mcmeta_unknown_format_warns() {
        let src = r#"{"pack":{"pack_format":6,"description":"x"}}"#;
        let r = migrate_pack_mcmeta(src, PackFormats::default(), false);
        assert_eq!(r.updated, src);
        assert_eq!(r.warnings.len(), 1);
    }
}
