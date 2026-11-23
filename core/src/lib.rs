//! UMMP core engine.
//!
//! Layered modular monolith: Domain → Application → Infrastructure.
//! No GUI, AI provider, or Minecraft loader dependency.

pub mod adapter;
pub mod ai;
pub mod analyzer;
pub mod app;
pub mod build;
pub mod build_script;
pub mod bytecode;
pub mod cache;
pub mod checkpoint;
pub mod config;
pub mod descriptor;
pub mod events;
pub mod filesystem;
pub mod jobs;
pub mod knowledge;
pub mod metadata;
pub mod migration;
pub mod mixin;
pub mod modpack;
pub mod persistence;
pub mod plugins;
pub mod port;
pub mod project;
pub mod resources;
pub mod scan;
pub mod session;

pub use knowledge::{BreakingChange, Compatibility, KnowledgeBase, Mapping, ModDependency, Recipe};
pub use project::ProjectManager;
