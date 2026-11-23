//! Data-driven detection: one [`RuleAdapter`] per `[[rule]]` in `adapter_rules.toml`.

use super::{AdapterContext, Handling, Level, TechAdapter, TechFinding};
use serde::Deserialize;
use std::path::Path;

const RULES: &str = include_str!("../adapter_rules.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub tech: String,
    pub level: Level,
    pub handling: Handling,
    pub message: String,
    /// File extensions (lowercase, without the dot).
    #[serde(default)]
    pub files: Vec<String>,
    /// Exact file names.
    #[serde(default)]
    pub names: Vec<String>,
    /// Text patterns, any of which must occur; empty means the file's presence is enough.
    #[serde(default)]
    pub any: Vec<String>,
}

#[derive(Deserialize)]
struct RuleFile {
    rule: Vec<Rule>,
}

struct Hit {
    line: Option<u32>,
}

pub struct RuleAdapter {
    rule: Rule,
}

impl RuleAdapter {
    /// One adapter per embedded rule.
    ///
    /// # Panics
    /// If the embedded rule file is invalid (caught by the test suite).
    #[must_use]
    pub fn all() -> Vec<Box<dyn TechAdapter>> {
        let file: RuleFile = toml::from_str(RULES).expect("embedded adapter_rules.toml is valid");
        file.rule
            .into_iter()
            .map(|rule| Box::new(Self { rule }) as Box<dyn TechAdapter>)
            .collect()
    }

    fn selects(&self, rel: &Path) -> bool {
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let ext = rel
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        self.rule.names.iter().any(|n| *n == name) || self.rule.files.iter().any(|e| *e == ext)
    }

    /// Whether the file matches, with the 1-based line of the first pattern hit
    /// (no line when the rule has no patterns).
    fn hit(&self, cx: &AdapterContext<'_>, rel: &Path) -> Option<Hit> {
        if self.rule.any.is_empty() {
            return Some(Hit { line: None });
        }
        let text = cx.text(rel)?;
        text.lines()
            .position(|l| self.rule.any.iter().any(|p| l.contains(p.as_str())))
            .map(|i| Hit {
                line: u32::try_from(i + 1).ok(),
            })
    }
}

impl TechAdapter for RuleAdapter {
    fn id(&self) -> &str {
        &self.rule.tech
    }

    fn level(&self) -> Level {
        self.rule.level
    }

    fn findings(&self, cx: &AdapterContext<'_>) -> Vec<TechFinding> {
        let mut hits = cx
            .files
            .iter()
            .filter(|rel| self.selects(rel))
            .filter_map(|rel| self.hit(cx, rel).map(|h| (rel, h.line)));
        let Some((first, line)) = hits.next() else {
            return Vec::new();
        };
        let others = hits.count();
        let more = if others > 0 {
            format!(" (+{others} more file(s))")
        } else {
            String::new()
        };
        vec![TechFinding {
            tech: self.rule.tech.clone(),
            handling: self.rule.handling,
            file: first.to_string_lossy().replace('\\', "/"),
            line,
            message: format!("{}{more}", self.rule.message),
        }]
    }
}
