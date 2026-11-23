//! The pack report: one row per mod with the action taken and the reason (guide §9).

use super::graph::PackIssue;
use super::plan::{PackAction, PackPlan};
use super::{EntryKind, Pack};
use serde::Serialize;
use std::fmt::Write as _;

#[derive(Debug, Clone, Serialize)]
pub struct ReportRow {
    pub mod_id: String,
    pub path: String,
    pub version: String,
    pub action: &'static str,
    pub reason: String,
    pub risk: String,
    pub technologies: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackReport {
    pub name: String,
    pub from: String,
    pub to: String,
    pub rows: Vec<ReportRow>,
    pub conflicts: Vec<String>,
    pub warnings: Vec<String>,
}

const fn action_name(a: &PackAction) -> &'static str {
    match a {
        PackAction::Keep => "keep",
        PackAction::Update(_) => "update",
        PackAction::Port { .. } => "port",
        PackAction::Replace { .. } => "replace",
        PackAction::Remove { .. } => "remove",
        PackAction::Manual { .. } => "manual",
    }
}

fn issue_text(i: &PackIssue) -> String {
    match i {
        PackIssue::MissingDependency { mod_id, requires } => {
            format!("{mod_id} requires {requires}, which is not in the pack")
        }
        PackIssue::DuplicateId { mod_id, paths } => {
            format!("{mod_id} appears more than once: {}", paths.join(", "))
        }
        PackIssue::Cycle { mods } => format!("dependency cycle: {}", mods.join(" -> ")),
        PackIssue::SideMismatch {
            mod_id,
            requires,
            side,
        } => format!("{mod_id} runs on the {side} but {requires} does not"),
    }
}

impl PackReport {
    /// One row per mod entry of the source pack, in path order.
    #[must_use]
    pub fn build(pack: &Pack, plan: &PackPlan, issues: &[PackIssue], warnings: &[String]) -> Self {
        let rows = pack
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Mod)
            .filter_map(|e| {
                let item = plan.items.iter().find(|i| i.path == e.path)?;
                Some(ReportRow {
                    mod_id: item.mod_id.clone(),
                    path: e.path.clone(),
                    version: e
                        .mod_info
                        .as_ref()
                        .map(|m| m.metadata.version.clone())
                        .unwrap_or_default(),
                    action: action_name(&item.action),
                    reason: item.reason.clone(),
                    risk: e.risk.map(|r| format!("{r:?}")).unwrap_or_default(),
                    technologies: e.technologies.clone(),
                })
            })
            .collect();
        Self {
            name: pack.name.clone(),
            from: format!(
                "{} {}",
                pack.mc.as_deref().unwrap_or("?"),
                pack.loader
                    .as_ref()
                    .map_or_else(|| "?".to_string(), |l| format!("{l:?}"))
            ),
            to: format!("{} {:?}", plan.target.mc, plan.target.loader),
            rows,
            conflicts: issues.iter().map(issue_text).collect(),
            warnings: warnings.to_vec(),
        }
    }

    #[must_use]
    pub fn count(&self, action: &str) -> usize {
        self.rows.iter().filter(|r| r.action == action).count()
    }

    #[must_use]
    pub fn to_markdown(&self) -> String {
        let cell = |s: &str| s.replace('|', "\\|").replace('\n', " ");
        let mut s = format!(
            "# UMMP pack report: {}\n\n{} -> {}\n\n",
            self.name, self.from, self.to
        );
        s.push_str("## Summary\n\n");
        for a in ["keep", "update", "port", "replace", "remove", "manual"] {
            let _ = writeln!(s, "- {a}: {}", self.count(a));
        }
        s.push_str("\n## Mods\n\n| Mod | File | Version | Action | Reason | Risk | Technologies |\n|---|---|---|---|---|---|---|\n");
        for r in &self.rows {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} | {} |",
                cell(&r.mod_id),
                cell(&r.path),
                cell(&r.version),
                r.action,
                cell(&r.reason),
                cell(&r.risk),
                cell(&r.technologies.join(", "))
            );
        }
        for (title, list) in [("Conflicts", &self.conflicts), ("Warnings", &self.warnings)] {
            if !list.is_empty() {
                let _ = write!(s, "\n## {title}\n\n");
                for l in list {
                    let _ = writeln!(s, "- {l}");
                }
            }
        }
        s
    }
}
