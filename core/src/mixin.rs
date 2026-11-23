//! Application: Mixin migration (Phase 6.E).
//!
//! - `*.mixins.json`: raises `compatibilityLevel` (`JAVA_N`) to the target's
//!   Java requirement.
//! - Mixin classes (source containing `@Mixin`): class renames are applied to
//!   the string forms Mixin uses and the source transformer skips: JVM
//!   descriptors (`Lnet/minecraft/Foo;`), internal names (`net/minecraft/Foo`)
//!   and dotted names in `targets = "..."`. Only qualified rules apply.
//! - Injector `method = "..."` targets need method mappings, which the KB does
//!   not hold yet: they are counted and reported for manual review.

use crate::descriptor::{apply_edits, find_key, string_value, EditResult, Edits};
use crate::migration::RenameRule;
use std::path::Path;

pub fn is_mixin_config(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".mixins.json") || n == "mixins.json")
}

pub fn migrate_mixin_config(content: &str, java: Option<u32>) -> EditResult {
    let mut result = EditResult {
        updated: content.to_string(),
        ..EditResult::default()
    };
    let (Some(java), Some(root)) = (java, content.find('{')) else {
        return result;
    };
    let Some(r) =
        find_key(content, root, "compatibilityLevel").and_then(|v| string_value(content, v))
    else {
        return result;
    };
    let old = &content[r.clone()];
    let current: Option<u32> = old.strip_prefix("JAVA_").and_then(|n| n.parse().ok());
    if current.is_some_and(|c| c < java) {
        let edits: Edits = vec![(r, old.to_string(), format!("JAVA_{java}"))];
        let valid = |t: &str| serde_json::from_str::<serde_json::Value>(t).is_ok();
        if let Some((updated, replacements)) = apply_edits(content, edits, valid) {
            result.updated = updated;
            result.replacements = replacements;
        }
    }
    result
}

pub fn is_mixin_class(content: &str) -> bool {
    content.contains("@Mixin")
}

/// Apply one stage of qualified rules to the Mixin string forms in `content`.
pub fn migrate_mixin_strings(content: &str, rules: &[RenameRule]) -> EditResult {
    let mut result = EditResult {
        updated: content.to_string(),
        ..EditResult::default()
    };
    let mut found: Edits = Vec::new();
    for rule in rules.iter().filter(|r| r.is_qualified()) {
        let (from_int, to_int) = (rule.from.replace('.', "/"), rule.to.replace('.', "/"));
        let patterns = [
            (format!("L{from_int};"), format!("L{to_int};")),
            (format!("\"{from_int}\""), format!("\"{to_int}\"")),
            (format!("\"{}\"", rule.from), format!("\"{}\"", rule.to)),
        ];
        for (pat, new) in patterns {
            for (pos, _) in content.match_indices(&pat) {
                found.push((pos..pos + pat.len(), pat.clone(), new.clone()));
            }
        }
    }
    // Drop overlapping matches (keep the earliest).
    found.sort_by_key(|(r, _, _)| r.start);
    let mut edits: Edits = Vec::new();
    for e in found {
        if edits
            .last()
            .is_none_or(|(last, _, _)| e.0.start >= last.end)
        {
            edits.push(e);
        }
    }
    if let Some((updated, replacements)) = apply_edits(content, edits, |_| true) {
        result.updated = updated;
        result.replacements = replacements;
    }
    result
}

/// A member mapping `pkg.Owner#name` → `pkg.Owner#newName` (KB kind `method`/`field`).
struct MemberRule<'a> {
    owner: &'a str,
    name: &'a str,
    new_name: &'a str,
}

fn member_rules(rules: &[RenameRule]) -> Vec<MemberRule<'_>> {
    rules
        .iter()
        .filter_map(|r| {
            let (owner, name) = r.from.split_once('#')?;
            let (_, new_name) = r.to.split_once('#')?;
            (name != new_name).then_some(MemberRule {
                owner,
                name,
                new_name,
            })
        })
        .collect()
}

/// `(start, end)` of the contents of the string literal opening at `open` (a `"`).
fn literal_at(content: &str, open: usize) -> Option<(usize, usize)> {
    let b = content.as_bytes();
    let mut i = open + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some((open + 1, i)),
            b'\n' => return None,
            _ => i += 1,
        }
    }
    None
}

/// Fully qualified classes named in `@Mixin(...)` (class literals resolved via imports or
/// the file's package, plus `targets = "..."` strings).
fn mixin_targets(content: &str) -> Vec<String> {
    let Some(start) = content.find("@Mixin(") else {
        return Vec::new();
    };
    let args = &content[start + 7..];
    let args = &args[..args.find(')').unwrap_or(args.len())];
    let package = content
        .lines()
        .find_map(|l| l.trim().strip_prefix("package "))
        .map(|p| p.trim_end_matches(';').trim().to_string());
    let resolve = |simple: &str| -> String {
        if simple.contains('.') {
            return simple.to_string();
        }
        content
            .lines()
            .filter_map(|l| l.trim().strip_prefix("import "))
            .map(|i| i.trim_end_matches(';').trim())
            .find(|i| i.rsplit('.').next() == Some(simple))
            .map_or_else(
                || {
                    package
                        .as_ref()
                        .map_or_else(|| simple.to_string(), |p| format!("{p}.{simple}"))
                },
                str::to_string,
            )
    };
    let mut out: Vec<String> = args
        .split(|c: char| c == ',' || c == '{' || c == '}' || c == '=' || c.is_whitespace())
        .filter_map(|t| t.strip_suffix(".class"))
        .map(resolve)
        .collect();
    let mut i = 0;
    while let Some(q) = args[i..].find('"') {
        let open = i + q;
        let Some((s, e)) = literal_at(args, open) else {
            break;
        };
        out.push(args[s..e].replace('/', "."));
        i = e + 1;
    }
    out
}

/// String literals that form the value of `key = ...` (a single literal or a `{...}` list).
fn values_of(content: &str, key: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (pos, _) in content.match_indices(key) {
        let before_ok = content[..pos]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let rest = &content[pos + key.len()..];
        let Some(after_eq) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        if !before_ok {
            continue;
        }
        let value_start = content.len() - after_eq.trim_start().len();
        match content[value_start..].chars().next() {
            Some('"') => out.extend(literal_at(content, value_start)),
            Some('{') => {
                let end = content[value_start..]
                    .find('}')
                    .map_or(content.len(), |e| value_start + e);
                let mut i = value_start;
                while let Some(q) = content[i..end].find('"') {
                    let Some(lit) = literal_at(content, i + q) else {
                        break;
                    };
                    out.push(lit);
                    i = lit.1 + 1;
                }
            }
            _ => {}
        }
    }
    out
}

/// The literal argument of `@Accessor("x")` / `@Invoker(value = "x")`.
fn annotation_value(content: &str, annotation: &str) -> Vec<(usize, usize)> {
    content
        .match_indices(annotation)
        .filter_map(|(pos, _)| {
            let inner = &content[pos + annotation.len()..];
            let close = inner.find(')')?;
            let q = inner[..close].find('"')?;
            literal_at(content, pos + annotation.len() + q)
        })
        .collect()
}

/// Applies method/field mappings (`pkg.Owner#name`) to a Mixin class.
///
/// Injector `method` values and `@Accessor`/`@Invoker` names change only when the `@Mixin`
/// target is the owner; `@At(target = "Lpkg/Owner;name(...)")` names its owner explicitly.
pub fn migrate_mixin_members(content: &str, rules: &[RenameRule]) -> EditResult {
    let mut result = EditResult {
        updated: content.to_string(),
        ..EditResult::default()
    };
    let members = member_rules(rules);
    if members.is_empty() || !is_mixin_class(content) {
        return result;
    }
    let targets = mixin_targets(content);
    let mut found: Edits = Vec::new();
    let mut rename_prefix = |(s, e): (usize, usize), m: &MemberRule<'_>| {
        let lit = &content[s..e];
        let rest = &lit[m.name.len().min(lit.len())..];
        if lit.starts_with(m.name) && (rest.is_empty() || rest.starts_with(['(', ':'])) {
            found.push((
                s..s + m.name.len(),
                m.name.to_string(),
                m.new_name.to_string(),
            ));
        }
    };
    for m in &members {
        if targets.iter().any(|t| t == m.owner) {
            for lit in values_of(content, "method") {
                rename_prefix(lit, m);
            }
            for a in ["@Accessor(", "@Invoker("] {
                for lit in annotation_value(content, a) {
                    rename_prefix(lit, m);
                }
            }
        }
        for lit in values_of(content, "target") {
            let owner_ref = format!("L{};", m.owner.replace('.', "/"));
            let (s, e) = lit;
            if let Some(p) = content[s..e].find(&owner_ref) {
                rename_prefix((s + p + owner_ref.len(), e), m);
            }
        }
    }
    found.sort_by_key(|(r, _, _)| r.start);
    found.dedup_by_key(|(r, _, _)| r.start);
    if let Some((updated, replacements)) = apply_edits(content, found, |_| true) {
        result.updated = updated;
        result.replacements = replacements;
    }
    result
}

/// Number of injector `method = "..."` targets, which need method mappings.
pub fn count_method_targets(content: &str) -> usize {
    content
        .match_indices("method")
        .filter(|(pos, _)| {
            let rest = content[pos + "method".len()..].trim_start();
            rest.strip_prefix('=')
                .is_some_and(|r| r.trim_start().starts_with(['"', '{']))
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(from: &str, to: &str) -> RenameRule {
        RenameRule {
            from: from.into(),
            to: to.into(),
            kind: "move".into(),
            confidence: 1.0,
            version: "1.21".into(),
        }
    }

    #[test]
    fn test_config_compatibility_level_raised_only() {
        let src = "{\n  \"required\": true,\n  \"compatibilityLevel\": \"JAVA_8\",\n  \"mixins\": []\n}\n";
        let r = migrate_mixin_config(src, Some(17));
        assert_eq!(r.updated, src.replace("JAVA_8", "JAVA_17"));
        assert_eq!(r.replacements[0].line, 3);
        let r = migrate_mixin_config(&src.replace("JAVA_8", "JAVA_21"), Some(17));
        assert!(r.replacements.is_empty());
    }

    #[test]
    fn test_mixin_strings_descriptor_internal_and_targets() {
        let src = r#"@Mixin(targets = "com.example.old.DemoWidget")
class DemoMixin {
    @Inject(method = "tick(Lcom/example/old/DemoWidget;I)V", at = @At("HEAD"))
    void a() {}
    @Redirect(method = "b", at = @At(value = "INVOKE", target = "com/example/old/DemoWidget"))
    void b() {}
    String unrelated = "Lcom/example/old/DemoWidgetExtra;";
}"#;
        let r = migrate_mixin_strings(
            src,
            &[
                rule("com.example.old.DemoWidget", "com.example.demo.DemoWidget"),
                rule("DemoWidget", "DemoGadget"), // simple rules never apply here
            ],
        );
        assert!(r
            .updated
            .contains(r#"targets = "com.example.demo.DemoWidget""#));
        assert!(r.updated.contains("tick(Lcom/example/demo/DemoWidget;I)V"));
        assert!(r
            .updated
            .contains(r#"target = "com/example/demo/DemoWidget""#));
        assert!(r.updated.contains("Lcom/example/old/DemoWidgetExtra;"));
        assert_eq!(r.replacements.len(), 3);
        assert_eq!(count_method_targets(src), 2);
    }

    #[test]
    fn test_detection() {
        assert!(is_mixin_config(Path::new(
            "src/main/resources/demo.mixins.json"
        )));
        assert!(!is_mixin_config(Path::new("demo.json")));
        assert!(is_mixin_class("@Mixin(Foo.class) class X {}"));
    }
}
