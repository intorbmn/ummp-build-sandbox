//! Domain: project metadata. No external dependencies.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Loader {
    Forge,
    Fabric,
    NeoForge,
    Quilt,
    BukkitsPaper,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinecraftVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl std::fmt::Display for MinecraftVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaVersion {
    pub major: u32,
    pub vendor: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModMetadata {
    pub mod_id: String,
    pub version: String,
    pub name: String,
    pub loader: Loader,
    pub minecraft: MinecraftVersion,
    pub java: Option<JavaVersion>,
    pub dependencies: Vec<ModDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModDependency {
    pub mod_id: String,
    pub version: String,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildSystem {
    pub kind: BuildSystemKind,
    pub file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildSystemKind {
    Gradle,
    Maven,
    Unknown(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub mods: Vec<ModMetadata>,
    pub build_system: Option<BuildSystem>,
    pub languages: Vec<String>,
}
