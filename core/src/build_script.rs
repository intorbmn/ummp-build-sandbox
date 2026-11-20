//! Application: build-script migration (Phase 6.C).
//!
//! Updates `gradle.properties` version keys and the Java level in
//! `build.gradle` / `build.gradle.kts`. Values change in place; everything else
//! in the file is kept. Keys whose new value UMMP cannot know (mappings,
//! Fabric API, Gradle plugin versions) are reported, never guessed.

use crate::descriptor::{
    apply_edits, push_edit, retarget_loader, retarget_minecraft, DescriptorTarget, EditResult,
    Edits,
};
use crate::migration::compare_versions;
use std::cmp::Ordering;
use std::path::Path;

/// Minimum Java release for a Minecraft version (Minecraft Wiki, "Java Edition"
/// system requirements). `None` for versions the table does not cover.
pub fn required_java(minecraft: &str) -> Option<u32> {
    let ge = |v: &str| {
        matches!(
            compare_versions(minecraft, v),
            Some(Ordering::Greater | Ordering::Equal)
        )
    };
    if compare_versions(minecraft, "1.0").is_none() {
        return None;
    }
    Some(if ge("26.1") {
        25
    } else if ge("1.20.5") {
        21
    } else if ge("1.18") {
        17
    } else if ge("1.17") {
        16
    } else {
        8
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildFile {
    GradleProperties,
    BuildScript,
}

impl BuildFile {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.file_name()?.to_str()? {
            "gradle.properties" => Some(Self::GradleProperties),
            "build.gradle" | "build.gradle.kts" => Some(Self::BuildScript),
            _ => None,
        }
    }
}

/// Keys reported for manual update: their target value is not derivable.
const MANUAL_KEYS: &[&str] = &[
    "mapping_version",
    "mappings_version",
    "yarn_mappings",
    "parchment_mappings_version",
    "parchment_minecraft_version",
    "fabric_version",
    "fabric_api_version",
    "qsl_version",
];

pub fn migrate_build_file(
    kind: BuildFile,
    content: &str,
    target: &DescriptorTarget,
    java: Option<u32>,
) -> EditResult {
    let mut result = EditResult {
        updated: content.to_string(),
        ..EditResult::default()
    };
    let edits = match kind {
        BuildFile::GradleProperties => properties_edits(content, target, &mut result.warnings),
        BuildFile::BuildScript => {
            let Some(java) = java else {
                result.warnings.push(format!(
                    "unknown Java requirement for Minecraft {}; Java level left unchanged",
                    target.minecraft
                ));
                return result;
            };
            java_edits(content, java)
        }
    };
    if let Some((updated, replacements)) = apply_edits(content, edits, |_| true) {
        result.updated = updated;
        result.replacements = replacements;
    }
    result
}

/// Value range of `key=value` (or `key = value`) on one properties line.
fn property(line: &str, base: usize) -> Option<(&str, std::ops::Range<usize>)> {
    let t = line.trim_start();
    if t.starts_with('#') || t.starts_with('!') {
        return None;
    }
    let eq = line.find(['=', ':'])?;
    let key = line[..eq].trim();
    let value_part = &line[eq + 1..];
    let lead = value_part.len() - value_part.trim_start().len();
    let value = value_part.trim();
    let start = base + eq + 1 + lead;
    Some((key, start..start + value.len()))
}

fn properties_edits(text: &str, target: &DescriptorTarget, warnings: &mut Vec<String>) -> Edits {
    let mut edits = Vec::new();
    let mc = target.minecraft.as_str();
    let lv = target.loader_version.as_deref();
    let loader_keys: &[&str] = match target.loader.as_str() {
        "forge" => &["forge_version"],
        "neoforge" => &["neo_version", "neoforge_version"],
        "fabric" | "quilt" => &["loader_version"],
        _ => &[],
    };
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if let Some((key, r)) = property(line, offset) {
            let old = &text[r.clone()];
            let new = match key {
                "minecraft_version" | "mc_version" => Some(mc.to_string()),
                "minecraft_version_range" => retarget_minecraft(old, mc),
                k if loader_keys.contains(&k) => match lv {
                    // Forge MDKs sometimes store "<mc>-<forge>".
                    Some(lv) if old.contains('-') => Some(format!("{mc}-{lv}")),
                    Some(lv) => Some(lv.to_string()),
                    None => {
                        warnings.push(format!(
                            "{key}: no {} version for Minecraft {mc} in the knowledge base",
                            target.loader
                        ));
                        None
                    }
                },
                "forge_version_range" | "neo_version_range" | "loader_version_range"
                    if target.loader == "forge" || key != "loader_version_range" =>
                {
                    lv.and_then(|lv| retarget_loader(old, lv))
                }
                k if MANUAL_KEYS.contains(&k) => {
                    warnings.push(format!(
                        "{key}={old}: update manually for Minecraft {mc} (not derivable)"
                    ));
                    None
                }
                _ => None,
            };
            push_edit(&mut edits, text, r, new);
        }
        offset += line.len();
    }
    edits
}

/// Raise every Java level written as `JavaLanguageVersion.of(N)`,
/// `JavaVersion.VERSION_N` / `VERSION_1_N`, or `release = N` /
/// `release.set(N)` to `java` when it is lower. Never lowers.
fn java_edits(text: &str, java: u32) -> Edits {
    let mut edits = Vec::new();
    let mut push_number = |range: std::ops::Range<usize>, value: u32, new: String| {
        if value < java {
            let old = text[range.clone()].to_string();
            edits.push((range, old, new));
        }
    };
    let digits_at = |i: usize| -> std::ops::Range<usize> {
        let len = text[i..]
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len() - i);
        i..i + len
    };

    for (pos, _) in text.match_indices("JavaLanguageVersion.of(") {
        let r = digits_at(pos + "JavaLanguageVersion.of(".len());
        if let Ok(v) = text[r.clone()].parse() {
            push_number(r, v, java.to_string());
        }
    }
    for (pos, _) in text.match_indices("JavaVersion.VERSION_") {
        let start = pos + "JavaVersion.VERSION_".len();
        let end = text[start..]
            .find(|c: char| !(c.is_ascii_digit() || c == '_'))
            .map_or(text.len(), |e| start + e);
        let token = &text[start..end];
        let v = token.strip_prefix("1_").unwrap_or(token).parse().ok();
        if let Some(v) = v {
            push_number(start..end, v, java.to_string());
        }
    }
    for (pos, _) in text.match_indices("release") {
        let rest = &text[pos + "release".len()..];
        let skip = rest.len()
            - rest
                .trim_start_matches(|c: char| c == ' ' || c == '=' || c == '.')
                .trim_start_matches("set(")
                .len();
        let start = pos + "release".len() + skip;
        if !text[pos + "release".len()..start].contains(['=', '(']) {
            continue;
        }
        let r = digits_at(start);
        if let Ok(v) = text[r.clone()].parse() {
            push_number(r, v, java.to_string());
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
    fn test_required_java_table() {
        assert_eq!(required_java("1.16.5"), Some(8));
        assert_eq!(required_java("1.17.1"), Some(16));
        assert_eq!(required_java("1.20.4"), Some(17));
        assert_eq!(required_java("1.20.5"), Some(21));
        assert_eq!(required_java("1.21.11"), Some(21));
        assert_eq!(required_java("26.1"), Some(25));
        assert_eq!(required_java("latest"), None);
    }

    #[test]
    fn test_forge_gradle_properties() {
        let src = "# MDK\nminecraft_version=1.16.5\nminecraft_version_range=[1.16.5,1.17)\n\
                   forge_version=36.2.39\nforge_version_range=[36,)\nloader_version_range=[36,)\n\
                   mapping_version=20210309-1.16.5\nmod_id=examplemod\n";
        let r = migrate_build_file(
            BuildFile::GradleProperties,
            src,
            &target("forge", "1.17.1", Some("37.1.1")),
            Some(16),
        );
        assert_eq!(
            r.updated,
            "# MDK\nminecraft_version=1.17.1\nminecraft_version_range=[1.17.1,1.18)\n\
             forge_version=37.1.1\nforge_version_range=[37,)\nloader_version_range=[37,)\n\
             mapping_version=20210309-1.16.5\nmod_id=examplemod\n"
        );
        assert_eq!(r.replacements.len(), 5);
        assert_eq!(r.warnings.len(), 1); // mapping_version
    }

    #[test]
    fn test_combined_forge_version_and_missing_loader() {
        let src = "forge_version = 1.16.5-36.2.39\n";
        let r = migrate_build_file(
            BuildFile::GradleProperties,
            src,
            &target("forge", "1.17.1", Some("37.1.1")),
            None,
        );
        assert_eq!(r.updated, "forge_version = 1.17.1-37.1.1\n");
        let r = migrate_build_file(
            BuildFile::GradleProperties,
            src,
            &target("forge", "1.17.1", None),
            None,
        );
        assert_eq!(r.updated, src);
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn test_fabric_loader_range_key_untouched() {
        // For Fabric, loader_version is the loader; there is no javafml range.
        let src =
            "minecraft_version=1.20.1\nloader_version=0.14.21\nfabric_version=0.86.0+1.20.1\n";
        let r = migrate_build_file(
            BuildFile::GradleProperties,
            src,
            &target("fabric", "1.21.1", Some("0.16.0")),
            Some(21),
        );
        assert!(r
            .updated
            .contains("minecraft_version=1.21.1\nloader_version=0.16.0\n"));
        assert!(r.updated.contains("fabric_version=0.86.0+1.20.1"));
        assert!(r.warnings[0].starts_with("fabric_version"));
    }

    #[test]
    fn test_java_levels_raised_never_lowered() {
        let src = "java.toolchain.languageVersion = JavaLanguageVersion.of(8)\n\
                   sourceCompatibility = JavaVersion.VERSION_1_8\n\
                   tasks.withType(JavaCompile).configureEach { it.options.release = 8 }\n\
                   kotlin { jvmToolchain(21) }\noptions.release.set(17)\n";
        let r = migrate_build_file(
            BuildFile::BuildScript,
            src,
            &target("forge", "1.17.1", None),
            Some(16),
        );
        assert_eq!(
            r.updated,
            "java.toolchain.languageVersion = JavaLanguageVersion.of(16)\n\
             sourceCompatibility = JavaVersion.VERSION_16\n\
             tasks.withType(JavaCompile).configureEach { it.options.release = 16 }\n\
             kotlin { jvmToolchain(21) }\noptions.release.set(17)\n"
        );
    }

    #[test]
    fn test_unknown_java_warns() {
        let r = migrate_build_file(
            BuildFile::BuildScript,
            "JavaLanguageVersion.of(17)",
            &target("fabric", "latest", None),
            None,
        );
        assert_eq!(r.updated, "JavaLanguageVersion.of(17)");
        assert_eq!(r.warnings.len(), 1);
    }
}
