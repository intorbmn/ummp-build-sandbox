//! Technology adapters (Phase 9, ADR 0012).
//!
//! Every technology a mod may use (other JVM languages, native code, shaders, loader APIs…)
//! is covered by a [`TechAdapter`]. An adapter always reports what it finds; some also
//! produce edits, which go through `MigrationEngine` like every other change.

pub mod loader;
pub mod native;
pub mod rules;

use crate::migration::{EditSet, MigrationError, MigrationPlan};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Who handles a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Handling {
    /// UMMP migrates it.
    Auto,
    /// Needs review; `ummp ai-assist` can propose fixes.
    Assisted,
    /// Must be ported by hand.
    Manual,
}

/// How far an adapter goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Detect,
    Report,
    Migrate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechFinding {
    pub tech: String,
    pub handling: Handling,
    pub file: String,
    pub line: Option<u32>,
    pub message: String,
}

/// One line per finding: `[handling] tech: message (file:line)`.
#[must_use]
pub fn findings_text(findings: &[TechFinding]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for f in findings {
        let at = f
            .line
            .map_or_else(|| f.file.clone(), |l| format!("{}:{l}", f.file));
        let handling = match f.handling {
            Handling::Auto => "auto",
            Handling::Assisted => "assisted",
            Handling::Manual => "manual",
        };
        let _ = writeln!(out, "- [{handling}] {}: {} ({at})", f.tech, f.message);
    }
    out
}

/// What an adapter may look at.
pub struct AdapterContext<'a> {
    pub root: &'a Path,
    /// Project files, relative to `root`.
    pub files: &'a [PathBuf],
    pub plan: Option<&'a MigrationPlan>,
    pub source_loader: Option<String>,
}

impl AdapterContext<'_> {
    /// File text, if it is a readable UTF-8 file of at most 2 MiB.
    #[must_use]
    pub fn text(&self, rel: &Path) -> Option<String> {
        let abs = crate::filesystem::resolve_within(self.root, rel).ok()?;
        if std::fs::metadata(&abs).ok()?.len() > 2 << 20 {
            return None;
        }
        String::from_utf8(std::fs::read(abs).ok()?).ok()
    }
}

pub trait TechAdapter: Send + Sync {
    fn id(&self) -> &str;
    fn level(&self) -> Level;
    fn findings(&self, cx: &AdapterContext<'_>) -> Vec<TechFinding>;
    /// Files this adapter rewrites itself; the engine's generic handlers skip them.
    fn owns(&self, _cx: &AdapterContext<'_>, _rel: &Path) -> bool {
        false
    }
    /// Rewrites a source file on top of the generic transforms (e.g. removing a loader
    /// interface); `None` leaves it unchanged.
    fn rewrite(&self, _cx: &AdapterContext<'_>, _rel: &Path, _text: &str) -> Option<String> {
        None
    }
    /// Edits for a migration (`cx.plan` is set).
    ///
    /// # Errors
    /// Reading project files failed.
    fn edits(&self, _cx: &AdapterContext<'_>) -> Result<EditSet, MigrationError> {
        Ok(EditSet::default())
    }
}

/// The adapters UMMP runs, in order.
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn TechAdapter>>,
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        let mut adapters: Vec<Box<dyn TechAdapter>> = Vec::new();
        adapters.extend(rules::RuleAdapter::all());
        adapters.push(Box::new(native::NativeAdapter));
        adapters.push(Box::new(loader::LoaderConverter::default()));
        Self { adapters }
    }
}

impl AdapterRegistry {
    #[must_use]
    pub fn new(adapters: Vec<Box<dyn TechAdapter>>) -> Self {
        Self { adapters }
    }

    #[must_use]
    pub fn adapters(&self) -> &[Box<dyn TechAdapter>] {
        &self.adapters
    }

    /// `(id, level)` of every adapter.
    #[must_use]
    pub fn list(&self) -> Vec<(String, Level)> {
        self.adapters
            .iter()
            .map(|a| (a.id().to_string(), a.level()))
            .collect()
    }

    #[must_use]
    pub fn findings(&self, cx: &AdapterContext<'_>) -> Vec<TechFinding> {
        self.adapters.iter().flat_map(|a| a.findings(cx)).collect()
    }

    #[must_use]
    pub fn owns(&self, cx: &AdapterContext<'_>, rel: &Path) -> bool {
        self.adapters.iter().any(|a| a.owns(cx, rel))
    }

    /// Runs every adapter's [`TechAdapter::rewrite`] in order.
    #[must_use]
    pub fn rewrite(&self, cx: &AdapterContext<'_>, rel: &Path, text: &str) -> String {
        self.adapters
            .iter()
            .fold(text.to_string(), |t, a| a.rewrite(cx, rel, &t).unwrap_or(t))
    }

    /// Findings for a project folder (used by the analyzer); empty for non-folders.
    #[must_use]
    pub fn scan_dir(&self, root: &Path) -> Vec<TechFinding> {
        if !root.is_dir() {
            return Vec::new();
        }
        let files = crate::checkpoint::collect_files(root).unwrap_or_default();
        self.findings(&AdapterContext {
            root,
            files: &files,
            plan: None,
            source_loader: None,
        })
    }
}
