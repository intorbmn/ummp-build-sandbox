//! Application: analyzer engine — adapter-based detectors behind IAnalyzer.

use crate::filesystem::{FileSystem, FsError};
use crate::metadata::{
    BuildSystem, BuildSystemKind, JavaVersion, Loader, MinecraftVersion, ModDependency, ModMetadata,
};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use zip::ZipArchive;

#[derive(Debug, Error)]
pub enum AnalyzerError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("fs: {0}")]
    Fs(#[from] FsError),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("parse: {0}")]
    Parse(String),
    #[error("not found: {0}")]
    NotFound(String),
}

pub trait IAnalyzer: Send + Sync {
    fn analyze(
        &self,
        project_root: &Path,
        fs: &Arc<dyn FileSystem>,
    ) -> Result<AnalysisReport, AnalyzerError>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub mods: Vec<ModAnalysis>,
    pub build_system: Option<BuildSystem>,
    pub languages: Vec<String>,
    pub dependency_graph: DependencyGraph,
    pub compatibility: CompatibilityMatrix,
    /// Technologies found by the adapters (Phase 9).
    #[serde(default)]
    pub technologies: Vec<crate::adapter::TechFinding>,
}

impl AnalysisReport {
    /// Machine-readable JSON export.
    pub fn to_json(&self) -> Result<String, AnalyzerError> {
        serde_json::to_string_pretty(self).map_err(|e| AnalyzerError::Parse(e.to_string()))
    }

    /// Human-readable summary text.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("=== Analysis Report ===\n");
        if let Some(ref bs) = self.build_system {
            let _ = writeln!(out, "Build system: {:?} ({})", bs.kind, bs.file.display());
        }
        if !self.languages.is_empty() {
            let _ = writeln!(out, "Languages: {}", self.languages.join(", "));
        }
        let _ = writeln!(out, "Mods: {}", self.mods.len());
        for m in &self.mods {
            let _ = writeln!(
                out,
                "  - {} {} [{:?}] mc={:?} java={:?}",
                m.metadata.name,
                m.metadata.version,
                m.detected_loader,
                m.metadata.minecraft,
                m.detected_java
            );
        }
        let _ = writeln!(
            out,
            "Dependency graph: {} nodes, {} edges",
            self.dependency_graph.nodes.len(),
            self.dependency_graph.edges.len()
        );
        if !self.compatibility.issues.is_empty() {
            out.push_str("Compatibility issues:\n");
            for i in &self.compatibility.issues {
                let _ = writeln!(out, "  [{:?}] {}: {}", i.severity, i.mod_id, i.message);
            }
        }
        if !self.technologies.is_empty() {
            out.push_str("Technologies:\n");
            out.push_str(&crate::adapter::findings_text(&self.technologies));
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModAnalysis {
    pub metadata: ModMetadata,
    pub source_files: Vec<PathBuf>,
    pub jar_path: Option<PathBuf>,
    pub detected_loader: Loader,
    pub detected_minecraft: MinecraftVersion,
    pub detected_java: Option<JavaVersion>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub nodes: Vec<DependencyNode>,
    pub edges: Vec<DependencyEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyNode {
    pub id: String,
    pub label: String,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyKind {
    Mod,
    Library,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    Requires,
    Optional,
    Conflicts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompatibilityMatrix {
    pub target_minecraft: String,
    pub target_loader: String,
    pub issues: Vec<CompatIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatIssue {
    pub mod_id: String,
    pub severity: IssueSeverity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
}

pub struct AnalyzerEngine {
    detectors: Vec<Box<dyn IAnalyzer>>,
}

impl AnalyzerEngine {
    pub fn new() -> Self {
        Self {
            detectors: Vec::new(),
        }
    }

    pub fn add_detector(&mut self, detector: Box<dyn IAnalyzer>) {
        self.detectors.push(detector);
    }

    pub fn analyze(
        &self,
        project_root: &Path,
        fs: &Arc<dyn FileSystem>,
    ) -> Result<AnalysisReport, AnalyzerError> {
        let mut report = AnalysisReport::default();
        for detector in &self.detectors {
            let partial = detector.analyze(project_root, &fs)?;
            report.mods.extend(partial.mods);
            if report.build_system.is_none() {
                report.build_system = partial.build_system;
            }
            for lang in partial.languages {
                if !report.languages.contains(&lang) {
                    report.languages.push(lang);
                }
            }
            report
                .dependency_graph
                .nodes
                .extend(partial.dependency_graph.nodes);
            report
                .dependency_graph
                .edges
                .extend(partial.dependency_graph.edges);
            report
                .compatibility
                .issues
                .extend(partial.compatibility.issues);
        }
        report.technologies = crate::adapter::AdapterRegistry::default().scan_dir(project_root);
        Ok(report)
    }
}

impl Default for AnalyzerEngine {
    fn default() -> Self {
        let mut engine = Self::new();
        engine.add_detector(Box::new(JarInspector));
        engine.add_detector(Box::new(SourceScanner));
        engine.add_detector(Box::new(BuildSystemDetector));
        engine.add_detector(Box::new(DependencyGraphBuilder));
        engine
    }
}

// 4.1 JAR inspection (manifest, mods.toml, fabric.mod.json)
pub struct JarInspector;

impl IAnalyzer for JarInspector {
    fn analyze(
        &self,
        project_root: &Path,
        fs: &Arc<dyn FileSystem>,
    ) -> Result<AnalysisReport, AnalyzerError> {
        let mut report = AnalysisReport::default();
        let jars = find_jars(project_root, fs.as_ref())?;

        for jar_path in jars {
            let mod_analysis = inspect_jar(&jar_path, fs.as_ref())?;
            report.mods.push(mod_analysis);
        }
        Ok(report)
    }
}

fn find_jars(root: &Path, fs: &dyn FileSystem) -> Result<Vec<PathBuf>, AnalyzerError> {
    let mut jars = Vec::new();
    for entry in fs.list(root)? {
        if entry.extension().is_some_and(|s| s == "jar") {
            jars.push(entry);
        }
    }
    Ok(jars)
}

/// Metadata descriptors read from a mod JAR. Only these entries are ever
/// decompressed: compiled classes and assets are binary and irrelevant.
#[derive(Debug, Default)]
struct JarMetadataFiles {
    manifest: String,
    mods_toml: String,
    neoforge_mods_toml: String,
    fabric_json: String,
    quilt_json: String,
    mcmod_info: String,
}

/// Cap per metadata entry, so a crafted archive cannot exhaust memory.
const MAX_METADATA_BYTES: u64 = 1024 * 1024;

pub fn inspect_jar(jar_path: &Path, fs: &dyn FileSystem) -> Result<ModAnalysis, AnalyzerError> {
    let bytes = fs.read(jar_path)?;
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor)?;

    let mut files = JarMetadataFiles::default();
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        // Normalize Windows backslashes to forward slashes (zip spec uses /)
        let name = file.name().replace('\\', "/");
        let slot = match name.as_str() {
            "META-INF/MANIFEST.MF" => &mut files.manifest,
            "META-INF/mods.toml" => &mut files.mods_toml,
            "META-INF/neoforge.mods.toml" => &mut files.neoforge_mods_toml,
            "fabric.mod.json" => &mut files.fabric_json,
            "quilt.mod.json" => &mut files.quilt_json,
            "mcmod.info" => &mut files.mcmod_info,
            _ => continue,
        };
        file.take(MAX_METADATA_BYTES).read_to_string(slot)?;
    }

    let (loader, metadata) = parse_jar_metadata(&files)?;
    let minecraft = detect_minecraft_version(&metadata);
    let java = detect_java_version(&files.manifest);

    Ok(ModAnalysis {
        metadata,
        source_files: Vec::new(),
        jar_path: Some(jar_path.to_path_buf()),
        detected_loader: loader,
        detected_minecraft: minecraft,
        detected_java: java,
    })
}

/// First dotted version inside a range expression:
/// `>=1.20.1` -> `1.20.1`, `[1.21,1.21.1)` -> `1.21`, `~1.20` -> `1.20`.
fn first_version(range: &str) -> Option<&str> {
    let start = range.find(|c: char| c.is_ascii_digit())?;
    let rest = &range[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    Some(rest[..end].trim_end_matches('.'))
}

fn parse_quilt_json(text: &str) -> Result<(Loader, ModMetadata), AnalyzerError> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| AnalyzerError::Parse(e.to_string()))?;
    let ql = &v["quilt_loader"];
    let mod_id = ql["id"].as_str().unwrap_or("unknown").to_string();
    let version = ql["version"].as_str().unwrap_or("0.0.0").to_string();
    let name = ql["metadata"]["name"]
        .as_str()
        .unwrap_or(&mod_id)
        .to_string();

    let mut minecraft = "1.20.1".to_string();
    let mut dependencies = Vec::new();
    for dep in ql["depends"].as_array().into_iter().flatten() {
        // A dependency is either a bare id string or an object.
        let (id, versions, optional) = match dep {
            serde_json::Value::String(id) => (id.as_str(), "*", false),
            obj => (
                obj["id"].as_str().unwrap_or(""),
                obj["versions"].as_str().unwrap_or("*"),
                obj["optional"].as_bool().unwrap_or(false),
            ),
        };
        if id == "minecraft" {
            if let Some(mc) = first_version(versions) {
                minecraft = mc.to_string();
            }
        } else if !id.is_empty() {
            dependencies.push(ModDependency {
                mod_id: id.to_string(),
                version: versions.to_string(),
                optional,
            });
        }
    }
    let (major, minor, patch) = parse_version(&minecraft);
    Ok((
        Loader::Quilt,
        ModMetadata {
            mod_id,
            version,
            name,
            loader: Loader::Quilt,
            minecraft: MinecraftVersion {
                major,
                minor,
                patch,
            },
            java: None,
            dependencies,
        },
    ))
}

fn parse_jar_metadata(files: &JarMetadataFiles) -> Result<(Loader, ModMetadata), AnalyzerError> {
    let fabric_json = files.fabric_json.as_str();
    let mcmod_info = files.mcmod_info.as_str();
    let manifest = files.manifest.as_str();
    // NeoForge 1.20.5+ uses neoforge.mods.toml; same schema as mods.toml.
    let (mods_toml, default_loader) = if files.neoforge_mods_toml.is_empty() {
        (files.mods_toml.as_str(), Loader::Forge)
    } else {
        (files.neoforge_mods_toml.as_str(), Loader::NeoForge)
    };

    // Try Fabric first
    if !fabric_json.is_empty() {
        let v: serde_json::Value =
            serde_json::from_str(fabric_json).map_err(|e| AnalyzerError::Parse(e.to_string()))?;
        let mod_id = v["id"].as_str().unwrap_or("unknown").to_string();
        let version = v["version"].as_str().unwrap_or("0.0.0").to_string();
        let name = v["name"].as_str().unwrap_or(&mod_id).to_string();
        let loader = Loader::Fabric;

        let minecraft = v["depends"]["minecraft"]
            .as_str()
            .and_then(first_version)
            .unwrap_or("1.20.1")
            .to_string();
        let (major, minor, patch) = parse_version(&minecraft);

        let mut dependencies = Vec::new();
        if let Some(deps) = v["depends"].as_object() {
            for (k, v) in deps {
                if k != "minecraft" {
                    dependencies.push(ModDependency {
                        mod_id: k.clone(),
                        version: v.as_str().unwrap_or("*").to_string(),
                        optional: false,
                    });
                }
            }
        }

        return Ok((
            loader.clone(),
            ModMetadata {
                mod_id,
                version,
                name,
                loader,
                minecraft: MinecraftVersion {
                    major,
                    minor,
                    patch,
                },
                java: None,
                dependencies,
            },
        ));
    }

    if !files.quilt_json.is_empty() {
        return parse_quilt_json(&files.quilt_json);
    }

    // Try Forge/NeoForge (mods.toml) — supports both flat and [[mods]] array formats
    if !mods_toml.is_empty() {
        let v: toml::Value = mods_toml
            .parse()
            .map_err(|e: toml::de::Error| AnalyzerError::Parse(e.to_string()))?;

        // Get the entry: prefer [[mods]] first element, fall back to flat table
        // Flat format (no [[mods]] array): modId/version/etc are at root level.
        let first = v
            .get("mods")
            .and_then(|m| m.as_array())
            .map_or(Some(&v), |mods_arr| mods_arr.first());

        if let Some(entry) = first {
            let mod_id = entry
                .get("modId")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let version = entry
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0.0")
                .to_string();
            let name = entry
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(&mod_id)
                .to_string();
            let loader = match entry.get("loader").and_then(|v| v.as_str()) {
                Some("NEOFORGE") => Loader::NeoForge,
                Some("FORGE") => Loader::Forge,
                Some(other) => Loader::Unknown(other.to_string()),
                None => default_loader,
            };

            // Real descriptors: `[[dependencies.<modId>]]` tables at the root.
            // Older fixtures: a `dependencies` array on the entry itself.
            let deps: &[toml::Value] = v
                .get("dependencies")
                .and_then(|d| d.get(&mod_id))
                .or_else(|| entry.get("dependencies"))
                .and_then(|d| d.as_array())
                .map_or(&[], Vec::as_slice);

            let mut minecraft_str = entry
                .get("minecraft")
                .and_then(|v| v.as_str())
                .unwrap_or("1.20.1")
                .to_string();
            let mut dependencies = Vec::new();
            for dep in deps {
                let id = dep.get("modId").and_then(|v| v.as_str()).unwrap_or("");
                let range = dep
                    .get("versionRange")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*");
                if id == "minecraft" {
                    if let Some(mc) = first_version(range) {
                        minecraft_str = mc.to_string();
                    }
                    continue;
                }
                let optional = dep.get("mandatory").and_then(toml::Value::as_bool) == Some(false)
                    || dep.get("type").and_then(|v| v.as_str()) == Some("optional");
                dependencies.push(ModDependency {
                    mod_id: id.to_string(),
                    version: range.to_string(),
                    optional,
                });
            }
            let (major, minor, patch) = parse_version(&minecraft_str);

            return Ok((
                loader.clone(),
                ModMetadata {
                    mod_id,
                    version,
                    name,
                    loader,
                    minecraft: MinecraftVersion {
                        major,
                        minor,
                        patch,
                    },
                    java: None,
                    dependencies,
                },
            ));
        }
    }

    // Try legacy mcmod.info
    if !mcmod_info.is_empty() {
        let v: serde_json::Value =
            serde_json::from_str(mcmod_info).map_err(|e| AnalyzerError::Parse(e.to_string()))?;
        let default_arr: Vec<serde_json::Value> = Vec::new();
        let arr = v.as_array().unwrap_or(&default_arr);
        if let Some(first) = arr.first() {
            let mod_id = first["modid"].as_str().unwrap_or("unknown").to_string();
            let version = first["version"].as_str().unwrap_or("0.0.0").to_string();
            let name = first["name"].as_str().unwrap_or(&mod_id).to_string();
            let mc_version = first["mcversion"].as_str().unwrap_or("1.20.1");
            let (major, minor, patch) = parse_version(mc_version);

            return Ok((
                Loader::Forge,
                ModMetadata {
                    mod_id,
                    version,
                    name,
                    loader: Loader::Forge,
                    minecraft: MinecraftVersion {
                        major,
                        minor,
                        patch,
                    },
                    java: None,
                    dependencies: Vec::new(),
                },
            ));
        }
    }

    // Fallback: parse MANIFEST.MF
    let mut mod_id = "unknown".to_string();
    let mut version = "0.0.0".to_string();
    let mut name = "Unknown Mod".to_string();

    for line in manifest.lines() {
        if line.starts_with("Implementation-Title:") {
            name = line["Implementation-Title:".len()..].trim().to_string();
        } else if line.starts_with("Implementation-Version:") {
            version = line["Implementation-Version:".len()..].trim().to_string();
        } else if line.starts_with("Mod-ID:") {
            mod_id = line["Mod-ID:".len()..].trim().to_string();
        }
    }

    Ok((
        Loader::Unknown("manifest".into()),
        ModMetadata {
            mod_id,
            version,
            name,
            loader: Loader::Unknown("manifest".into()),
            minecraft: MinecraftVersion {
                major: 1,
                minor: 20,
                patch: 1,
            },
            java: None,
            dependencies: Vec::new(),
        },
    ))
}

/// Parse `major.minor[.patch]`. A missing patch is 0 (`1.21` is `1.21.0`).
/// Unparseable input falls back to 1.20.1 (see KNOWN_LIMITATIONS.md).
fn parse_version(s: &str) -> (u32, u32, u32) {
    let parts: Vec<Option<u32>> = s.split('.').map(|p| p.parse().ok()).collect();
    match parts.as_slice() {
        [Some(major), Some(minor)] => (*major, *minor, 0),
        [Some(major), Some(minor), Some(patch), ..] => (*major, *minor, *patch),
        _ => (1, 20, 1),
    }
}

fn detect_minecraft_version(metadata: &ModMetadata) -> MinecraftVersion {
    metadata.minecraft.clone()
}

fn detect_java_version(manifest: &str) -> Option<JavaVersion> {
    for line in manifest.lines() {
        if line.starts_with("Build-Jdk:") || line.starts_with("Created-By:") {
            let val = line.split(':').nth(1).unwrap_or("").trim();
            if val.contains("17") || val.contains("21") {
                let major = if val.contains("21") { 21 } else { 17 };
                return Some(JavaVersion {
                    major,
                    vendor: "unknown".into(),
                });
            }
        }
    }
    None
}

// 4.2 Source tree scan
pub struct SourceScanner;

impl IAnalyzer for SourceScanner {
    fn analyze(
        &self,
        project_root: &Path,
        fs: &Arc<dyn FileSystem>,
    ) -> Result<AnalysisReport, AnalyzerError> {
        let mut report = AnalysisReport::default();
        let java_files = find_files(project_root, fs.as_ref(), "java")?;
        let kt_files = find_files(project_root, fs.as_ref(), "kt")?;
        let scala_files = find_files(project_root, fs.as_ref(), "scala")?;

        if !java_files.is_empty() {
            report.languages.push("java".into());
        }
        if !kt_files.is_empty() {
            report.languages.push("kotlin".into());
        }
        if !scala_files.is_empty() {
            report.languages.push("scala".into());
        }

        // Source projects keep their descriptors under src/main/resources (Phase 9).
        let res = project_root.join("src/main/resources");
        let read = |rel: &str| -> String {
            let p = res.join(rel);
            if fs.exists(&p) {
                fs.read(&p)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
                    .unwrap_or_default()
            } else {
                String::new()
            }
        };
        let files = JarMetadataFiles {
            manifest: String::new(),
            mods_toml: read("META-INF/mods.toml"),
            neoforge_mods_toml: read("META-INF/neoforge.mods.toml"),
            fabric_json: read("fabric.mod.json"),
            quilt_json: read("quilt.mod.json"),
            mcmod_info: read("mcmod.info"),
        };
        let any = [
            &files.mods_toml,
            &files.neoforge_mods_toml,
            &files.fabric_json,
            &files.quilt_json,
            &files.mcmod_info,
        ]
        .iter()
        .any(|s| !s.is_empty());
        if any {
            if let Ok((loader, metadata)) = parse_jar_metadata(&files) {
                let minecraft = detect_minecraft_version(&metadata);
                report.mods.push(ModAnalysis {
                    metadata,
                    source_files: java_files.iter().chain(&kt_files).cloned().collect(),
                    jar_path: None,
                    detected_loader: loader,
                    detected_minecraft: minecraft,
                    detected_java: None,
                });
            }
        }

        Ok(report)
    }
}

fn find_files(root: &Path, fs: &dyn FileSystem, ext: &str) -> Result<Vec<PathBuf>, AnalyzerError> {
    let mut files = Vec::new();
    for entry in fs.list(root)? {
        if entry.extension().is_some_and(|s| s == ext) {
            files.push(entry);
        } else if fs.is_dir(&entry) {
            files.extend(find_files(&entry, fs, ext)?);
        }
    }
    Ok(files)
}

// 4.5 Build system detection
pub struct BuildSystemDetector;

impl IAnalyzer for BuildSystemDetector {
    fn analyze(
        &self,
        project_root: &Path,
        fs: &Arc<dyn FileSystem>,
    ) -> Result<AnalysisReport, AnalyzerError> {
        let mut report = AnalysisReport::default();

        let gradle_files = [
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
        ];
        let maven_files = ["pom.xml"];

        for file in gradle_files {
            let path = project_root.join(file);
            if fs.exists(&path) {
                report.build_system = Some(BuildSystem {
                    kind: BuildSystemKind::Gradle,
                    file: path,
                });
                break;
            }
        }

        if report.build_system.is_none() {
            for file in maven_files {
                let path = project_root.join(file);
                if fs.exists(&path) {
                    report.build_system = Some(BuildSystem {
                        kind: BuildSystemKind::Maven,
                        file: path,
                    });
                    break;
                }
            }
        }

        Ok(report)
    }
}

// 4.4 Dependency graph builder
pub struct DependencyGraphBuilder;

impl IAnalyzer for DependencyGraphBuilder {
    fn analyze(
        &self,
        project_root: &Path,
        fs: &Arc<dyn FileSystem>,
    ) -> Result<AnalysisReport, AnalyzerError> {
        let mut report = AnalysisReport::default();
        let jars = find_jars(project_root, fs.as_ref())?;
        let mut seen = std::collections::HashSet::new();

        for jar_path in &jars {
            let analysis = inspect_jar(jar_path, fs.as_ref())?;
            let id = analysis.metadata.mod_id.clone();
            if !seen.insert(id.clone()) {
                continue;
            }
            report.dependency_graph.nodes.push(DependencyNode {
                id: id.clone(),
                label: analysis.metadata.name.clone(),
                kind: DependencyKind::Mod,
            });
            for dep in &analysis.metadata.dependencies {
                report.dependency_graph.edges.push(DependencyEdge {
                    from: id.clone(),
                    to: dep.mod_id.clone(),
                    kind: if dep.optional {
                        EdgeKind::Optional
                    } else {
                        EdgeKind::Requires
                    },
                });
                if seen.insert(dep.mod_id.clone()) {
                    report.dependency_graph.nodes.push(DependencyNode {
                        id: dep.mod_id.clone(),
                        label: dep.mod_id.clone(),
                        kind: DependencyKind::Library,
                    });
                }
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::LocalFileSystem;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_analyzer_engine_default() {
        let engine = AnalyzerEngine::default();
        assert_eq!(engine.detectors.len(), 4);
    }

    #[test]
    fn test_find_jars_empty() {
        let dir = tempdir().unwrap();
        let fs = LocalFileSystem;
        let jars = find_jars(dir.path(), &fs).unwrap();
        assert_eq!(jars.len(), 0, "unexpected jars: {jars:?}");
    }

    #[test]
    fn test_build_system_detector_gradle() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("build.gradle"), "plugins { id 'java' }").unwrap();
        let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
        let detector = BuildSystemDetector;
        let report = detector.analyze(dir.path(), &fs).unwrap();
        assert!(matches!(
            report.build_system,
            Some(BuildSystem {
                kind: BuildSystemKind::Gradle,
                ..
            })
        ));
    }

    #[test]
    fn test_build_system_detector_maven() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("pom.xml"), "<project></project>").unwrap();
        let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
        let detector = BuildSystemDetector;
        let report = detector.analyze(dir.path(), &fs).unwrap();
        assert!(matches!(
            report.build_system,
            Some(BuildSystem {
                kind: BuildSystemKind::Maven,
                ..
            })
        ));
    }

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("1.20.1"), (1, 20, 1));
        // 1.21 and 1.21.1 are different releases.
        assert_eq!(parse_version("1.21"), (1, 21, 0));
        assert_eq!(parse_version("invalid"), (1, 20, 1));
        assert_eq!(parse_version("garbage"), (1, 20, 1));
    }

    #[test]
    fn test_dependency_graph_builder_empty() {
        let dir = tempdir().unwrap();
        let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
        let detector = DependencyGraphBuilder;
        let report = detector.analyze(dir.path(), &fs).unwrap();
        assert!(report.dependency_graph.nodes.is_empty());
        assert!(report.dependency_graph.edges.is_empty());
    }

    #[test]
    fn test_report_export_json_and_text() {
        let report = AnalysisReport::default();
        let json = report.to_json().unwrap();
        assert!(json.contains("\"mods\""));
        let text = report.to_text();
        assert!(text.contains("Analysis Report"));
    }

    /// Integration test over the fixture mod corpus (task 4.7).
    /// Fixtures live at `../tests/fixtures` relative to the crate root.
    #[test]
    fn test_analyze_fixture_corpus() {
        let fixtures = Path::new("../tests/fixtures");
        let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
        let engine = AnalyzerEngine::default();
        let report = engine.analyze(fixtures, &fs).unwrap();

        // 12 fixture JARs: fabric-a/b/c, fabric-classes, forge-a/b, neoforge-a/b,
        // quilt-a, legacy-a, manifest-a, empty
        assert_eq!(report.mods.len(), 12, "expected 12 fixture mods");

        let ids: Vec<&str> = report
            .mods
            .iter()
            .map(|m| m.metadata.mod_id.as_str())
            .collect();
        for id in [
            "fabrica",
            "fabricb",
            "fabricc",
            "forgea",
            "neoforgea",
            "forgeb",
            "legacya",
            "manifestmod",
        ] {
            assert!(ids.contains(&id), "missing mod id: {id}");
        }

        // Loader detection
        let loader = |id: &str| {
            report
                .mods
                .iter()
                .find(|m| m.metadata.mod_id == id)
                .map(|m| m.detected_loader.clone())
                .unwrap()
        };
        assert!(matches!(loader("fabrica"), Loader::Fabric));
        assert!(matches!(loader("neoforgea"), Loader::NeoForge));
        assert!(matches!(loader("forgea"), Loader::Forge));
        assert!(matches!(loader("legacya"), Loader::Forge));
        assert!(matches!(loader("manifestmod"), Loader::Unknown(_)));

        // Real-world shapes: binary .class entries, neoforge.mods.toml, quilt.mod.json
        assert!(matches!(loader("fabricclasses"), Loader::Fabric));
        assert!(matches!(loader("neoforgeb"), Loader::NeoForge));
        assert!(matches!(loader("quilta"), Loader::Quilt));
        let mc = |id: &str| {
            let m = report
                .mods
                .iter()
                .find(|m| m.metadata.mod_id == id)
                .unwrap();
            m.metadata.minecraft.to_string()
        };
        assert_eq!(mc("neoforgeb"), "1.21.0");
        assert_eq!(mc("quilta"), "1.20.1");
        let quilt = report
            .mods
            .iter()
            .find(|m| m.metadata.mod_id == "quilta")
            .unwrap();
        let quilt_deps: Vec<&str> = quilt
            .metadata
            .dependencies
            .iter()
            .map(|d| d.mod_id.as_str())
            .collect();
        assert_eq!(quilt_deps, ["quilt_loader", "qsl"]);

        // Real-world shapes: binary .class entries, neoforge.mods.toml, quilt.mod.json
        assert!(matches!(loader("fabricclasses"), Loader::Fabric));
        assert!(matches!(loader("neoforgeb"), Loader::NeoForge));
        assert!(matches!(loader("quilta"), Loader::Quilt));
        let mc = |id: &str| {
            let m = report
                .mods
                .iter()
                .find(|m| m.metadata.mod_id == id)
                .unwrap();
            m.metadata.minecraft.to_string()
        };
        assert_eq!(mc("neoforgeb"), "1.21.0");
        assert_eq!(mc("quilta"), "1.20.1");
        let quilt = report
            .mods
            .iter()
            .find(|m| m.metadata.mod_id == "quilta")
            .unwrap();
        let quilt_deps: Vec<&str> = quilt
            .metadata
            .dependencies
            .iter()
            .map(|d| d.mod_id.as_str())
            .collect();
        assert_eq!(quilt_deps, ["quilt_loader", "qsl"]);

        // Java version from manifest
        let manifestmod = report
            .mods
            .iter()
            .find(|m| m.metadata.mod_id == "manifestmod")
            .unwrap();
        assert_eq!(
            manifestmod.detected_java.as_ref().map(|j| j.major),
            Some(17)
        );

        // Languages: kotlin-src has Example.kt
        assert!(report.languages.contains(&"kotlin".to_string()));

        // Dependency graph built from mod metadata
        assert!(!report.dependency_graph.nodes.is_empty());
        assert!(!report.dependency_graph.edges.is_empty());

        // Exports work end-to-end
        let _json = report.to_json().unwrap();
        let text = report.to_text();
        assert!(text.contains("Fabrica"));
        assert!(text.contains("NeoForgeA"));
    }

    #[test]
    fn source_tree_descriptor_is_read() {
        let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/loader");
        let fabric = AnalyzerEngine::default()
            .analyze(&root.join("fabric-src"), &fs)
            .unwrap();
        assert_eq!(fabric.mods.len(), 1);
        assert_eq!(fabric.mods[0].metadata.mod_id, "demowidget");
        assert_eq!(fabric.mods[0].metadata.loader, Loader::Fabric);
        assert!(fabric.mods[0].jar_path.is_none());
        let neo = AnalyzerEngine::default()
            .analyze(&root.join("neoforge-src"), &fs)
            .unwrap();
        assert_eq!(neo.mods[0].metadata.loader, Loader::NeoForge);
    }
}
