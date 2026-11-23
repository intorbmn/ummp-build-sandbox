//! Loader conversion (Phase 9, ADR 0012): Fabric/Quilt ↔ Forge ↔ NeoForge.
//!
//! Structural only: descriptor, entry-point bridge, Mixin and access-widener/transformer
//! registration, and package renames between loaders. Loader API *semantics* are not
//! rewritten; every remaining use of the source loader's API is reported as `assisted`.

use super::{AdapterContext, Handling, Level, TechAdapter, TechFinding};
use crate::descriptor::DescriptorTarget;
use crate::migration::{
    EditSet, FileEdit, MigrationError, RenameRule, SourceTransformer, TokenRenameTransformer,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const DATA: &str = include_str!("../loader_rules.toml");

#[derive(Deserialize)]
struct Data {
    prefixes: BTreeMap<String, Vec<String>>,
    rename: Vec<RenameRow>,
}

#[derive(Deserialize)]
struct RenameRow {
    pair: String,
    from: String,
    to: String,
}

const TEMPLATES: &str = include_str!("../loader_templates.toml");

/// Build files for one `(loader, minecraft)` pair.
#[derive(Deserialize)]
pub struct Template {
    pub loader: String,
    pub minecraft: String,
    pub gradle: String,
    pub settings_gradle: String,
    pub gradle_properties: String,
    pub build_gradle: String,
}

#[derive(Deserialize)]
struct TemplateFile {
    template: Vec<Template>,
}

fn all_templates() -> &'static TemplateFile {
    static T: OnceLock<TemplateFile> = OnceLock::new();
    T.get_or_init(|| toml::from_str(TEMPLATES).expect("embedded loader_templates.toml is valid"))
}

/// `(loader, minecraft)` of every shipped build template, in file order.
#[must_use]
pub fn templates() -> Vec<(&'static str, &'static str)> {
    all_templates()
        .template
        .iter()
        .map(|t| (t.loader.as_str(), t.minecraft.as_str()))
        .collect()
}

/// The build template for `loader` (family) and `minecraft`, if UMMP ships one.
#[must_use]
pub fn template(loader: &str, minecraft: &str) -> Option<&'static Template> {
    all_templates()
        .template
        .iter()
        .find(|t| t.loader == loader && t.minecraft == minecraft)
}

const BUILD_FILES: &[&str] = &[
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "gradle.properties",
    "gradle/wrapper/gradle-wrapper.properties",
];

/// `key=value` from a properties text.
fn property(text: &str, keys: &[&str]) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        keys.contains(&k.trim())
            .then(|| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

/// Lines of the old `dependencies { ... }` block that are not loader/Minecraft setup,
/// as comments for the user to review.
fn carried_dependencies(build: &str) -> String {
    const LOADER: &[&str] = &[
        "minecraft",
        "mappings",
        "fabric-loader",
        "fabric-api",
        "neoforge",
        "net.minecraftforge",
        "net.neoforged",
        "net.fabricmc",
        "forge",
    ];
    let mut out = Vec::new();
    let mut depth = 0_i32;
    let mut inside = false;
    for line in build.lines() {
        let t = line.trim();
        if !inside && t.starts_with("dependencies") && t.ends_with('{') {
            inside = true;
            depth = 1;
            continue;
        }
        if inside {
            depth += i32::try_from(t.matches('{').count()).unwrap_or(0);
            depth -= i32::try_from(t.matches('}').count()).unwrap_or(0);
            if depth <= 0 {
                inside = false;
                continue;
            }
            if !t.is_empty() && !t.starts_with("//") && !LOADER.iter().any(|w| t.contains(w)) {
                out.push(format!("    // UMMP: from the old build, review: {t}"));
            }
        }
    }
    if out.is_empty() {
        "    // UMMP: no other dependencies in the old build".into()
    } else {
        out.join("\n")
    }
}

fn data() -> &'static Data {
    static D: OnceLock<Data> = OnceLock::new();
    D.get_or_init(|| toml::from_str(DATA).expect("embedded loader_rules.toml is valid"))
}

/// Loader family: Quilt reads Fabric descriptors and APIs.
#[must_use]
pub fn family(loader: &str) -> Option<&'static str> {
    match loader.to_ascii_lowercase().as_str() {
        "fabric" | "quilt" => Some("fabric"),
        "forge" => Some("forge"),
        "neoforge" => Some("neoforge"),
        _ => None,
    }
}

fn display(family: &str) -> &'static str {
    match family {
        "fabric" => "Fabric",
        "forge" => "Forge",
        _ => "NeoForge",
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dep {
    pub id: String,
    pub range: String,
    pub required: bool,
}

/// Loader-independent view of a mod descriptor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModInfo {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: String,
    /// Fully qualified main class (Fabric `main` entrypoint or the `@Mod` class).
    pub entry: Option<String>,
    pub mixins: Vec<String>,
    pub deps: Vec<Dep>,
    /// Access widener (Fabric) or access transformer (Forge) path, relative to resources.
    pub access: Option<String>,
}

fn str_or_obj(v: &Value, key: &str) -> Option<String> {
    v.as_str()
        .map(str::to_string)
        .or_else(|| v.get(key).and_then(Value::as_str).map(str::to_string))
}

fn first_str(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::to_string)
        .or_else(|| v.as_array()?.first()?.as_str().map(str::to_string))
}

/// Parses `fabric.mod.json`.
#[must_use]
pub fn parse_fabric(text: &str) -> Option<ModInfo> {
    let v: Value = serde_json::from_str(text).ok()?;
    let s = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let list = |k: &str, inner: &str| -> Vec<String> {
        v.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| str_or_obj(x, inner)).collect())
            .unwrap_or_default()
    };
    let id = s("id");
    if id.is_empty() {
        return None;
    }
    let deps = v
        .get("depends")
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .map(|(id, r)| Dep {
                    id: id.clone(),
                    range: first_str(r).unwrap_or_else(|| "*".into()),
                    required: true,
                })
                .collect()
        })
        .unwrap_or_default();
    Some(ModInfo {
        name: v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string(),
        version: s("version"),
        description: s("description"),
        authors: list("authors", "name"),
        license: v.get("license").and_then(first_str).unwrap_or_default(),
        entry: v
            .pointer("/entrypoints/main/0")
            .and_then(|e| str_or_obj(e, "value")),
        mixins: list("mixins", "config"),
        access: v
            .get("accessWidener")
            .and_then(Value::as_str)
            .map(str::to_string),
        deps,
        id,
    })
}

fn toml_str(v: &toml::Value, k: &str) -> String {
    v.get(k)
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Parses `META-INF/mods.toml` / `neoforge.mods.toml` (the entry class comes from `@Mod`).
#[must_use]
pub fn parse_mods_toml(text: &str) -> Option<ModInfo> {
    let t: toml::Value = toml::from_str(text).ok()?;
    let m = t.get("mods")?.as_array()?.first()?;
    let id = toml_str(m, "modId");
    let tables = |k: &str| {
        t.get(k)
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let deps = t
        .get("dependencies")
        .and_then(|d| d.get(&id))
        .and_then(toml::Value::as_array)
        .map(|a| {
            a.iter()
                .map(|d| Dep {
                    id: toml_str(d, "modId"),
                    range: toml_str(d, "versionRange"),
                    required: d.get("type").and_then(toml::Value::as_str).map_or_else(
                        || {
                            d.get("mandatory")
                                .and_then(toml::Value::as_bool)
                                .unwrap_or(true)
                        },
                        |ty| ty == "required",
                    ),
                })
                .collect()
        })
        .unwrap_or_default();
    let authors = toml_str(m, "authors");
    let name = toml_str(m, "displayName");
    Some(ModInfo {
        name: if name.is_empty() { id.clone() } else { name },
        version: toml_str(m, "version"),
        description: toml_str(m, "description"),
        authors: if authors.is_empty() {
            vec![]
        } else {
            vec![authors]
        },
        license: toml_str(&t, "license"),
        entry: None,
        mixins: tables("mixins")
            .iter()
            .map(|x| toml_str(x, "config"))
            .filter(|c| !c.is_empty())
            .collect(),
        access: tables("accessTransformers")
            .first()
            .map(|x| toml_str(x, "file")),
        deps,
        id,
    })
}

/// Fabric/semver range → Maven range.
fn to_maven(r: &str) -> String {
    let r = r.trim();
    if r.is_empty() || r == "*" {
        return "[0,)".into();
    }
    if let Some(v) = r
        .strip_prefix(">=")
        .or_else(|| r.strip_prefix('~'))
        .or_else(|| r.strip_prefix('^'))
    {
        return format!("[{},)", v.trim());
    }
    if let Some(v) = r.strip_prefix('>') {
        return format!("({},)", v.trim());
    }
    if r.starts_with(|c: char| c.is_ascii_digit()) && !r.contains(' ') {
        return format!("[{r}]");
    }
    "[0,)".into()
}

/// Maven range → Fabric/semver range.
fn to_fabric(r: &str) -> String {
    let r = r.trim();
    if let Some(v) = r.strip_prefix('[').and_then(|x| x.strip_suffix(",)")) {
        return format!(">={v}");
    }
    match r.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        Some(v) if !v.contains(',') => v.to_string(),
        _ => "*".into(),
    }
}

fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

const FABRIC_ONLY_DEPS: &[&str] = &[
    "fabricloader",
    "fabric",
    "fabric-api",
    "java",
    "minecraft",
    "quilt_loader",
    "quilted_fabric_api",
];
const FORGE_ONLY_DEPS: &[&str] = &["forge", "neoforge", "minecraft"];

fn kept_deps<'a>(info: &'a ModInfo, from: &str) -> impl Iterator<Item = &'a Dep> {
    let skip = if from == "fabric" {
        FABRIC_ONLY_DEPS
    } else {
        FORGE_ONLY_DEPS
    };
    info.deps
        .iter()
        .filter(move |d| !skip.contains(&d.id.as_str()))
}

fn license(info: &ModInfo) -> &str {
    if info.license.is_empty() {
        "All Rights Reserved"
    } else {
        &info.license
    }
}

/// `META-INF/mods.toml` (Forge) or `neoforge.mods.toml` (NeoForge) for `info`.
#[must_use]
pub fn write_mods_toml(
    info: &ModInfo,
    target: &DescriptorTarget,
    neoforge: bool,
    source: &str,
    from: &str,
) -> String {
    let version = if info.version.contains("${") {
        "${file.jarVersion}".to_string()
    } else {
        info.version.clone()
    };
    let mut blocks = vec![format!(
        "# Converted from {source} by UMMP.\nmodLoader = \"javafml\"\nloaderVersion = \"[1,)\"\nlicense = {}\n",
        q(license(info))
    )];
    let mut mods = format!(
        "[[mods]]\nmodId = {}\nversion = {}\ndisplayName = {}\ndescription = '''\n{}\n'''\n",
        q(&info.id),
        q(&version),
        q(&info.name),
        info.description.trim()
    );
    if !info.authors.is_empty() {
        let _ = writeln!(mods, "authors = {}", q(&info.authors.join(", ")));
    }
    blocks.push(mods);
    if neoforge {
        for m in &info.mixins {
            blocks.push(format!("[[mixins]]\nconfig = {}\n", q(m)));
        }
        if info.access.is_some() {
            blocks
                .push("[[accessTransformers]]\nfile = \"META-INF/accesstransformer.cfg\"\n".into());
        }
    }
    let dep = |id: &str, range: &str, required: bool| {
        let need = if neoforge {
            format!(
                "type = \"{}\"",
                if required { "required" } else { "optional" }
            )
        } else {
            format!("mandatory = {required}")
        };
        format!(
            "[[dependencies.{}]]\nmodId = {}\n{need}\nversionRange = {}\nordering = \"NONE\"\nside = \"BOTH\"\n",
            info.id,
            q(id),
            q(range)
        )
    };
    let loader_range = target
        .loader_version
        .as_ref()
        .map_or_else(|| "[0,)".to_string(), |v| format!("[{v},)"));
    blocks.push(dep(
        if neoforge { "neoforge" } else { "forge" },
        &loader_range,
        true,
    ));
    blocks.push(dep("minecraft", &format!("[{},)", target.minecraft), true));
    for d in kept_deps(info, from) {
        let range = if from == "fabric" {
            to_maven(&d.range)
        } else {
            d.range.clone()
        };
        blocks.push(dep(&d.id, &range, d.required));
    }
    blocks.join("\n")
}

/// `fabric.mod.json` for `info` with `entry` as the main entrypoint.
#[must_use]
pub fn write_fabric(
    info: &ModInfo,
    target: &DescriptorTarget,
    java: Option<u32>,
    entry: &str,
    from: &str,
) -> String {
    let j = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let arr = |v: &[String]| {
        format!(
            "[{}]",
            v.iter().map(|s| j(s)).collect::<Vec<_>>().join(", ")
        )
    };
    let version = if info.version.contains("${") {
        concat!("$", "{version}").to_string()
    } else {
        info.version.clone()
    };
    let mut lines = vec![
        "  \"schemaVersion\": 1".to_string(),
        format!("  \"id\": {}", j(&info.id)),
        format!("  \"version\": {}", j(&version)),
        format!("  \"name\": {}", j(&info.name)),
        format!("  \"description\": {}", j(info.description.trim())),
    ];
    if !info.authors.is_empty() {
        lines.push(format!("  \"authors\": {}", arr(&info.authors)));
    }
    lines.push(format!("  \"license\": {}", j(license(info))));
    lines.push("  \"environment\": \"*\"".into());
    lines.push(format!(
        "  \"entrypoints\": {{\n    \"main\": [{}]\n  }}",
        j(entry)
    ));
    if !info.mixins.is_empty() {
        lines.push(format!("  \"mixins\": {}", arr(&info.mixins)));
    }
    if info.access.is_some() {
        lines.push(format!(
            "  \"accessWidener\": {}",
            j(&format!("{}.accesswidener", info.id))
        ));
    }
    let loader = target
        .loader_version
        .as_ref()
        .map_or_else(|| "*".to_string(), |v| format!(">={v}"));
    let mut deps = vec![
        format!("    \"fabricloader\": {}", j(&loader)),
        format!(
            "    \"minecraft\": {}",
            j(&format!("~{}", target.minecraft))
        ),
    ];
    if let Some(java) = java {
        deps.push(format!("    \"java\": {}", j(&format!(">={java}"))));
    }
    for d in kept_deps(info, from).filter(|d| d.required) {
        deps.push(format!("    {}: {}", j(&d.id), j(&to_fabric(&d.range))));
    }
    lines.push(format!("  \"depends\": {{\n{}\n  }}", deps.join(",\n")));
    format!("{{\n{}\n}}\n", lines.join(",\n"))
}

fn tokens(line: &str) -> Vec<&str> {
    line.split('#')
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect()
}

/// Access widener (Fabric) → access transformer ((Neo)Forge).
fn widener_to_transformer(text: &str, source: &str) -> String {
    let mut out = vec![format!("# Converted from {source} by UMMP.")];
    for line in text.lines().skip(1) {
        let t = tokens(line);
        if t.len() < 3 {
            continue;
        }
        let access = t[0].trim_start_matches("transitive-");
        let owner = t[2].replace('/', ".");
        let modifier = if access == "accessible" {
            "public"
        } else {
            "public-f"
        };
        out.push(match (t[1], t.get(3), t.get(4)) {
            ("method", Some(n), Some(d)) => format!("{modifier} {owner} {n}{d}"),
            ("field", Some(n), _) => format!("{modifier} {owner} {n}"),
            _ => format!("{modifier} {owner}"),
        });
    }
    out.join("\n") + "\n"
}

/// Access transformer → access widener; fields need a type descriptor AT files lack.
fn transformer_to_widener(text: &str, source: &str) -> (String, usize) {
    let mut out = vec![
        "accessWidener v2 named".to_string(),
        format!("# Converted from {source} by UMMP."),
    ];
    let mut unresolved = 0;
    for line in text.lines() {
        let t = tokens(line);
        if t.len() < 2 {
            continue;
        }
        let final_removed = t[0].ends_with("-f");
        let owner = t[1].replace('.', "/");
        match t.get(2) {
            None => {
                out.push(format!("accessible class {owner}"));
                if final_removed {
                    out.push(format!("extendable class {owner}"));
                }
            }
            Some(member) => match member.find('(') {
                Some(p) => out.push(format!(
                    "accessible method {owner} {} {}",
                    &member[..p],
                    &member[p..]
                )),
                None => {
                    unresolved += 1;
                    let kind = if final_removed {
                        "mutable"
                    } else {
                        "accessible"
                    };
                    out.push(format!(
                        "# UMMP: add the field type descriptor: {kind} field {owner} {member} <descriptor>"
                    ));
                }
            },
        }
    }
    (out.join("\n") + "\n", unresolved)
}

/// Drops lines for which `drop(trimmed line, next non-blank trimmed line)` is true.
fn remove_lines(text: &str, drop: impl Fn(&str, Option<&str>) -> bool) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    lines
        .iter()
        .enumerate()
        .filter(|(i, l)| {
            let next = lines[i + 1..]
                .iter()
                .find(|n| !n.trim().is_empty())
                .map(|n| n.trim());
            !drop(l.trim(), next)
        })
        .map(|(_, l)| *l)
        .collect()
}

/// Removes Fabric's `ModInitializer` (import, `implements`, `@Override` on `onInitialize`).
fn strip_mod_initializer(text: &str) -> String {
    let text = text.replace("net.fabricmc.api.ModInitializer", "ModInitializer");
    let text = remove_lines(&text, |l, next| {
        l == "import ModInitializer;"
            || (l == "@Override" && next.is_some_and(|n| n.contains("onInitialize(")))
    });
    text.replace("implements ModInitializer, ", "implements ")
        .replace(", ModInitializer", "")
        .replace(" implements ModInitializer", "")
}

/// Removes the (Neo)Forge `@Mod` annotation and its import.
fn strip_mod_annotation(text: &str) -> String {
    remove_lines(text, |l, _| {
        l.starts_with("@Mod(")
            || l == "import net.neoforged.fml.common.Mod;"
            || l == "import net.minecraftforge.fml.common.Mod;"
    })
}

/// Everything about one conversion, computed once per project.
#[derive(Clone)]
struct Conv {
    from: &'static str,
    to: &'static str,
    descriptor: PathBuf,
    resources: PathBuf,
    info: ModInfo,
    /// Source file of the main class.
    entry_file: Option<PathBuf>,
    /// Access widener/transformer file to convert (relative to the project).
    access_file: Option<PathBuf>,
    renames: Vec<RenameRule>,
}

fn package_and_class(text: &str, rel: &Path) -> Option<String> {
    let class = rel.file_stem()?.to_string_lossy().into_owned();
    let pkg = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("package "))?;
    Some(format!("{}.{class}", pkg.trim_end_matches(';').trim()))
}

fn is_source(rel: &Path) -> bool {
    matches!(
        rel.extension().and_then(|e| e.to_str()),
        Some("java" | "kt" | "scala" | "groovy")
    )
}

fn is_descriptor_of(from: &str, rel: &Path) -> bool {
    let name = rel
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let in_meta = rel
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|p| p == "META-INF");
    match from {
        "fabric" => name == "fabric.mod.json",
        "forge" => name == "mods.toml" && in_meta,
        _ => name == "neoforge.mods.toml" && in_meta,
    }
}

fn conversion(cx: &AdapterContext<'_>) -> Option<Conv> {
    let plan = cx.plan?;
    let from = family(cx.source_loader.as_deref()?)?;
    let to = family(&plan.request.target_loader)?;
    if from == to {
        return None;
    }
    let descriptor = cx
        .files
        .iter()
        .filter(|r| is_descriptor_of(from, r))
        .min_by_key(|r| r.components().count())?
        .clone();
    let text = cx.text(&descriptor)?;
    let mut info = if from == "fabric" {
        parse_fabric(&text)?
    } else {
        parse_mods_toml(&text)?
    };
    let resources = if from == "fabric" {
        descriptor.parent()?.to_path_buf()
    } else {
        descriptor.parent()?.parent()?.to_path_buf()
    };
    let sources: Vec<&PathBuf> = cx.files.iter().filter(|r| is_source(r)).collect();
    let entry_file = if from == "fabric" {
        info.entry.as_ref().and_then(|e| {
            let tail = e.replace('.', "/");
            sources
                .iter()
                .find(|r| {
                    r.with_extension("")
                        .to_string_lossy()
                        .replace('\\', "/")
                        .ends_with(&tail)
                })
                .map(|r| (*r).clone())
        })
    } else {
        sources.iter().find_map(|r| {
            let t = cx.text(r)?;
            t.lines()
                .any(|l| l.trim_start().starts_with("@Mod("))
                .then(|| {
                    info.entry = package_and_class(&t, r);
                    (*r).clone()
                })
        })
    };
    let access_file = if from == "fabric" {
        info.access.as_ref().map(|a| resources.join(a))
    } else {
        Some(
            resources.join(
                info.access
                    .clone()
                    .unwrap_or_else(|| "META-INF/accesstransformer.cfg".into()),
            ),
        )
    }
    .filter(|p| cx.files.contains(p));
    info.access = access_file
        .as_ref()
        .map(|_| "META-INF/accesstransformer.cfg".to_string());
    let pair = format!("{from}->{to}");
    let renames = data()
        .rename
        .iter()
        .filter(|r| r.pair == pair)
        .map(|r| RenameRule {
            from: r.from.clone(),
            to: r.to.clone(),
            kind: "loader".into(),
            confidence: 1.0,
            version: pair.clone(),
        })
        .collect();
    Some(Conv {
        from,
        to,
        descriptor,
        resources,
        info,
        entry_file,
        access_file,
        renames,
    })
}

type CacheKey = (PathBuf, Option<String>, String);

/// Converts a mod between loader families.
#[derive(Default)]
pub struct LoaderConverter {
    cache: Mutex<Option<(CacheKey, Option<Conv>)>>,
}

impl LoaderConverter {
    fn conv(&self, cx: &AdapterContext<'_>) -> Option<Conv> {
        let key: CacheKey = (
            cx.root.to_path_buf(),
            cx.source_loader.clone(),
            cx.plan
                .map(|p| p.request.target_loader.clone())
                .unwrap_or_default(),
        );
        let mut cache = self.cache.lock().ok()?;
        if let Some((k, c)) = cache.as_ref() {
            if *k == key {
                return c.clone();
            }
        }
        let c = conversion(cx);
        *cache = Some((key, c.clone()));
        c
    }

    fn template_of(cx: &AdapterContext<'_>, c: &Conv) -> Option<&'static Template> {
        template(c.to, &cx.plan?.descriptor_target.minecraft)
    }

    /// Replaces the build files from the template for the target loader and version.
    fn build_edits(cx: &AdapterContext<'_>, c: &Conv, t: &Template, set: &mut EditSet) {
        let old_props = cx.text(Path::new("gradle.properties")).unwrap_or_default();
        let old_build = ["build.gradle", "build.gradle.kts"]
            .iter()
            .find_map(|f| cx.text(Path::new(f)))
            .unwrap_or_default();
        let version = Some(c.info.version.clone())
            .filter(|v| !v.is_empty() && !v.contains("${"))
            .or_else(|| property(&old_props, &["mod_version", "version"]))
            .unwrap_or_else(|| "1.0.0".into());
        let group = property(&old_props, &["maven_group", "mod_group_id", "group"])
            .or_else(|| {
                c.info
                    .entry
                    .as_ref()
                    .and_then(|e| e.rsplit_once('.').map(|(p, _)| p.to_string()))
            })
            .unwrap_or_else(|| "com.example".into());
        let access = if c.to == "fabric" && c.access_file.is_some() {
            let res = c.resources.to_string_lossy().replace('\\', "/");
            format!(
                "\nloom {{\n    accessWidenerPath = file(\"{res}/{}.accesswidener\")\n}}\n",
                c.info.id
            )
        } else {
            String::new()
        };
        let fill = |s: &str| {
            s.replace("@MOD_ID@", &c.info.id)
                .replace("@VERSION@", &version)
                .replace("@GROUP@", &group)
                .replace("@USER_DEPS@", &carried_dependencies(&old_build))
                .replace("@ACCESS@", &access)
        };
        let mut put = |name: &str, kts: Option<&str>, text: String| {
            if let Some(k) = kts.filter(|k| cx.files.iter().any(|f| f == Path::new(k))) {
                set.deletes.push(PathBuf::from(k));
            }
            let path = PathBuf::from(name);
            let original = cx.text(&path).unwrap_or_default();
            if original != text {
                set.edits.push(FileEdit {
                    path,
                    original,
                    updated: text,
                    replacements: vec![],
                });
            }
        };
        put(
            "build.gradle",
            Some("build.gradle.kts"),
            fill(&t.build_gradle),
        );
        put(
            "settings.gradle",
            Some("settings.gradle.kts"),
            fill(&t.settings_gradle),
        );
        put("gradle.properties", None, fill(&t.gradle_properties));
        let wrapper = Path::new("gradle/wrapper/gradle-wrapper.properties");
        if let Some(text) = cx.text(wrapper) {
            let url = format!(
                "distributionUrl=https\\://services.gradle.org/distributions/gradle-{}-bin.zip",
                t.gradle
            );
            let updated: String = text
                .split_inclusive('\n')
                .map(|l| {
                    if l.starts_with("distributionUrl=") {
                        format!("{url}\n")
                    } else {
                        l.to_string()
                    }
                })
                .collect();
            put(&wrapper.to_string_lossy(), None, updated);
        }
    }

    fn rewrite_with(c: &Conv, rel: &Path, text: &str) -> String {
        let mut out = if c.renames.is_empty() {
            text.to_string()
        } else {
            TokenRenameTransformer.transform(text, &c.renames).0
        };
        if c.entry_file.as_deref() == Some(rel) {
            if c.from == "fabric" {
                out = strip_mod_initializer(&out);
            } else if c.to == "fabric" {
                out = strip_mod_annotation(&out);
            }
        }
        out
    }
}

fn finding(handling: Handling, file: &Path, line: Option<u32>, message: String) -> TechFinding {
    TechFinding {
        tech: "loader-conversion".into(),
        handling,
        file: file.to_string_lossy().replace('\\', "/"),
        line,
        message,
    }
}

const fn create(path: PathBuf, updated: String) -> FileEdit {
    FileEdit {
        path,
        original: String::new(),
        updated,
        replacements: vec![],
    }
}

/// Source of the generated entry class for `(from, to)`, if one is needed.
fn bridge(c: &Conv, pkg: &str, simple: &str, ctor_args: bool) -> Option<(&'static str, String)> {
    let id = &c.info.id;
    match (c.from, c.to) {
        ("fabric", "neoforge") => Some((
            "UmmpNeoForgeEntry",
            format!(
                "package {pkg};\n\nimport net.neoforged.bus.api.IEventBus;\nimport net.neoforged.fml.common.Mod;\n\n\
                 /** Generated by UMMP: NeoForge entry point that runs the former Fabric initializer. */\n\
                 @Mod(\"{id}\")\npublic final class UmmpNeoForgeEntry {{\n    public UmmpNeoForgeEntry(IEventBus modBus) {{\n        new {simple}().onInitialize();\n    }}\n}}\n"
            ),
        )),
        ("fabric", "forge") => Some((
            "UmmpForgeEntry",
            format!(
                "package {pkg};\n\nimport net.minecraftforge.fml.common.Mod;\n\n\
                 /** Generated by UMMP: Forge entry point that runs the former Fabric initializer. */\n\
                 @Mod(\"{id}\")\npublic final class UmmpForgeEntry {{\n    public UmmpForgeEntry() {{\n        new {simple}().onInitialize();\n    }}\n}}\n"
            ),
        )),
        (_, "fabric") => {
            let body = if ctor_args {
                format!("        // UMMP: {simple}'s constructor takes loader arguments; port its setup here by hand.\n")
            } else {
                format!("        new {simple}();\n")
            };
            Some((
                "UmmpFabricEntry",
                format!(
                    "package {pkg};\n\nimport net.fabricmc.api.ModInitializer;\n\n\
                     /** Generated by UMMP: Fabric entry point that runs the former {} mod constructor. */\n\
                     public final class UmmpFabricEntry implements ModInitializer {{\n    @Override\n    public void onInitialize() {{\n{body}    }}\n}}\n",
                    display(c.from)
                ),
            ))
        }
        _ => None,
    }
}

impl TechAdapter for LoaderConverter {
    fn id(&self) -> &'static str {
        "loader-conversion"
    }

    fn level(&self) -> Level {
        Level::Migrate
    }

    fn owns(&self, cx: &AdapterContext<'_>, rel: &Path) -> bool {
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        let build = BUILD_FILES.contains(&rel_s.as_str());
        let candidate = build
            || matches!(
                name.as_ref(),
                "fabric.mod.json" | "mods.toml" | "neoforge.mods.toml" | "accesstransformer.cfg"
            )
            || name.ends_with(".accesswidener");
        candidate
            && self.conv(cx).is_some_and(|c| {
                c.descriptor == rel
                    || (build && Self::template_of(cx, &c).is_some())
                    || (c.access_file.as_deref() == Some(rel)
                        && (c.from == "fabric" || c.to == "fabric"))
            })
    }

    fn rewrite(&self, cx: &AdapterContext<'_>, rel: &Path, text: &str) -> Option<String> {
        let c = self.conv(cx)?;
        let out = Self::rewrite_with(&c, rel, text);
        (out != text).then_some(out)
    }

    fn findings(&self, cx: &AdapterContext<'_>) -> Vec<TechFinding> {
        let Some(c) = self.conv(cx) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let prefixes = data().prefixes.get(c.from).cloned().unwrap_or_default();
        for rel in cx.files.iter().filter(|r| is_source(r)) {
            let Some(text) = cx.text(rel) else { continue };
            let text = Self::rewrite_with(&c, rel, &text);
            for (i, line) in text.lines().enumerate() {
                if prefixes.iter().any(|p| line.contains(p.as_str())) {
                    out.push(finding(
                        Handling::Assisted,
                        rel,
                        u32::try_from(i + 1).ok(),
                        format!(
                            "{} API left after conversion: `{}`; port it (ummp ai-assist can help)",
                            display(c.from),
                            line.trim()
                        ),
                    ));
                }
            }
        }
        if c.info
            .deps
            .iter()
            .any(|d| d.id == "fabric-api" || d.id == "fabric")
        {
            out.push(finding(
                Handling::Manual,
                &c.descriptor,
                None,
                "dependency 'fabric-api' dropped: port its API calls (listed as assisted)".into(),
            ));
        }
        for d in kept_deps(&c.info, c.from) {
            out.push(finding(
                Handling::Manual,
                &c.descriptor,
                None,
                format!(
                    "dependency '{}' kept: check that it exists for {}",
                    d.id,
                    display(c.to)
                ),
            ));
        }
        if c.entry_file.is_none() {
            out.push(finding(
                Handling::Manual,
                &c.descriptor,
                None,
                "main class not found: write the entry point by hand".into(),
            ));
        }
        let minecraft = cx
            .plan
            .map(|p| p.descriptor_target.minecraft.clone())
            .unwrap_or_default();
        match Self::template_of(cx, &c) {
            None => out.push(finding(
                Handling::Manual,
                Path::new("build.gradle"),
                None,
                format!(
                    "no build template for {} {minecraft}: convert build.gradle and gradle.properties by hand",
                    display(c.to)
                ),
            )),
            Some(t) if !cx.files.iter().any(|f| f == Path::new("gradle/wrapper/gradle-wrapper.properties")) => {
                out.push(finding(
                    Handling::Manual,
                    Path::new("build.gradle"),
                    None,
                    format!(
                        "no Gradle wrapper: add one for Gradle {} (`gradle wrapper --gradle-version {}`)",
                        t.gradle, t.gradle
                    ),
                ));
            }
            Some(_) => {}
        }
        if c.to == "forge" && !c.info.mixins.is_empty() {
            out.push(finding(
                Handling::Assisted,
                &c.descriptor,
                None,
                "Forge reads Mixin configs from the jar manifest: add `MixinConfigs` in the build script".into(),
            ));
        }
        out
    }

    fn edits(&self, cx: &AdapterContext<'_>) -> Result<EditSet, MigrationError> {
        let mut set = EditSet::default();
        let (Some(c), Some(plan)) = (self.conv(cx), cx.plan) else {
            return Ok(set);
        };
        let source_name = c
            .descriptor
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut info = c.info.clone();
        if let Some(access) = &c.access_file {
            let text = cx.text(access).unwrap_or_default();
            let rel_name = access
                .strip_prefix(&c.resources)
                .unwrap_or(access)
                .to_string_lossy()
                .replace('\\', "/");
            if c.from == "fabric" {
                set.edits.push(create(
                    c.resources.join("META-INF/accesstransformer.cfg"),
                    widener_to_transformer(&text, &rel_name),
                ));
                set.deletes.push(access.clone());
            } else if c.to == "fabric" {
                let (aw, unresolved) = transformer_to_widener(&text, &rel_name);
                set.edits.push(create(
                    c.resources.join(format!("{}.accesswidener", info.id)),
                    aw,
                ));
                set.deletes.push(access.clone());
                if unresolved > 0 {
                    set.warnings.push(format!(
                        "{unresolved} access-transformer field(s) need a type descriptor in the access widener"
                    ));
                }
            }
        }
        let mut entry = info.entry.clone().unwrap_or_default();
        if let (Some(file), Some(main)) = (&c.entry_file, &info.entry) {
            let (pkg, simple) = main.rsplit_once('.').unwrap_or(("", main));
            let text = cx.text(file).unwrap_or_default();
            let ctor_args =
                text.contains(&format!("{simple}(")) && !text.contains(&format!("{simple}()"));
            if c.to == "fabric" && ctor_args {
                set.warnings.push(format!(
                    "{simple}'s constructor takes loader arguments: port its setup into UmmpFabricEntry by hand"
                ));
            }
            if let Some((class, src)) = bridge(&c, pkg, simple, ctor_args) {
                let path = file
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(format!("{class}.java"));
                if cx.files.contains(&path) {
                    set.warnings.push(format!(
                        "{}: already exists, entry bridge not written",
                        path.display()
                    ));
                } else {
                    set.edits.push(create(path, src));
                    if c.to == "fabric" {
                        entry = format!("{pkg}.{class}");
                    }
                }
            }
        }
        info.entry = Some(entry.clone());
        if let Some(t) = Self::template_of(cx, &c) {
            Self::build_edits(cx, &c, t, &mut set);
        }
        let target = &plan.descriptor_target;
        let (target_rel, text) = match c.to {
            "fabric" => (
                c.resources.join("fabric.mod.json"),
                write_fabric(&info, target, plan.java, &entry, c.from),
            ),
            "forge" => (
                c.resources.join("META-INF/mods.toml"),
                write_mods_toml(&info, target, false, &source_name, c.from),
            ),
            _ => (
                c.resources.join("META-INF/neoforge.mods.toml"),
                write_mods_toml(&info, target, true, &source_name, c.from),
            ),
        };
        if cx.files.contains(&target_rel) {
            set.warnings.push(format!(
                "{}: already exists, descriptor not converted",
                target_rel.display()
            ));
        } else {
            set.edits.push(create(target_rel, text));
            set.deletes.push(c.descriptor);
        }
        Ok(set)
    }
}
