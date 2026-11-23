//! Dependency graph of a pack: missing required dependencies, duplicate ids, cycles and
//! side mismatches. Built only from entries whose descriptor was read.

use super::{PackEntry, Support};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Ids that name the game or the loader, never a mod in the pack.
pub const BUILTIN_IDS: &[&str] = &[
    "minecraft",
    "java",
    "fabricloader",
    "fabric-loader",
    "fabric_loader",
    "quilt_loader",
    "quiltloader",
    "forge",
    "neoforge",
    "fml",
    "javafml",
];

/// An id another mod provides under a second name (`fabric` is provided by `fabric-api`).
const ALIASES: &[(&str, &str)] = &[("fabric", "fabric-api")];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackIssue {
    MissingDependency {
        mod_id: String,
        requires: String,
    },
    DuplicateId {
        mod_id: String,
        paths: Vec<String>,
    },
    Cycle {
        mods: Vec<String>,
    },
    SideMismatch {
        mod_id: String,
        requires: String,
        /// `"client"` or `"server"`: the side where `mod_id` runs but `requires` does not.
        side: &'static str,
    },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PackGraph {
    /// mod id → required ids that are present in the pack.
    pub requires: BTreeMap<String, Vec<String>>,
    pub issues: Vec<PackIssue>,
}

fn is_builtin(id: &str) -> bool {
    BUILTIN_IDS.contains(&id)
}

impl PackGraph {
    #[must_use]
    pub fn build(entries: &[PackEntry]) -> Self {
        // id -> entries carrying it (only entries with a readable, known descriptor).
        let mut by_id: BTreeMap<&str, Vec<&PackEntry>> = BTreeMap::new();
        for e in entries {
            if let Some(m) = &e.mod_info {
                let id = m.metadata.mod_id.as_str();
                if id != "unknown" {
                    by_id.entry(id).or_default().push(e);
                }
            }
        }
        let present = |id: &str| -> Option<&str> {
            if by_id.contains_key(id) {
                return by_id.get_key_value(id).map(|(k, _)| *k);
            }
            ALIASES
                .iter()
                .find(|(a, _)| *a == id)
                .and_then(|(_, to)| by_id.get_key_value(to).map(|(k, _)| *k))
        };

        let mut g = Self::default();
        for (id, es) in &by_id {
            if es.len() > 1 {
                let mut paths: Vec<String> = es.iter().map(|e| e.path.clone()).collect();
                paths.sort();
                g.issues.push(PackIssue::DuplicateId {
                    mod_id: (*id).to_string(),
                    paths,
                });
            }
        }

        for (id, es) in &by_id {
            let mut reqs = BTreeSet::new();
            for e in es {
                let Some(m) = &e.mod_info else { continue };
                for d in m.metadata.dependencies.iter().filter(|d| !d.optional) {
                    if is_builtin(&d.mod_id) || d.mod_id == *id {
                        continue;
                    }
                    match present(&d.mod_id) {
                        Some(found) => {
                            reqs.insert(found.to_string());
                            for (side, mine, theirs) in [
                                ("client", e.env.client, side_of(&by_id, found, true)),
                                ("server", e.env.server, side_of(&by_id, found, false)),
                            ] {
                                if mine != Support::Unsupported && theirs == Support::Unsupported {
                                    g.issues.push(PackIssue::SideMismatch {
                                        mod_id: (*id).to_string(),
                                        requires: found.to_string(),
                                        side,
                                    });
                                }
                            }
                        }
                        None => g.issues.push(PackIssue::MissingDependency {
                            mod_id: (*id).to_string(),
                            requires: d.mod_id.clone(),
                        }),
                    }
                }
            }
            g.requires
                .insert((*id).to_string(), reqs.into_iter().collect());
        }
        g.issues.extend(cycles(&g.requires));
        g.issues.sort_by(|a, b| issue_key(a).cmp(&issue_key(b)));
        g.issues.dedup();
        g
    }
}

/// The support level of `id` on one side; a duplicate is as supported as its best copy.
fn side_of(by_id: &BTreeMap<&str, Vec<&PackEntry>>, id: &str, client: bool) -> Support {
    let mut best = Support::Unsupported;
    for e in by_id.get(id).into_iter().flatten() {
        let s = if client { e.env.client } else { e.env.server };
        if s == Support::Required {
            return s;
        }
        if s == Support::Optional {
            best = s;
        }
    }
    best
}

fn issue_key(i: &PackIssue) -> (u8, String, String) {
    match i {
        PackIssue::DuplicateId { mod_id, .. } => (0, mod_id.clone(), String::new()),
        PackIssue::MissingDependency { mod_id, requires } => (1, mod_id.clone(), requires.clone()),
        PackIssue::SideMismatch {
            mod_id,
            requires,
            side,
        } => (2, mod_id.clone(), format!("{requires}/{side}")),
        PackIssue::Cycle { mods } => (3, mods.join(","), String::new()),
    }
}

/// Strongly connected groups of two or more mods, found without recursion.
fn cycles(requires: &BTreeMap<String, Vec<String>>) -> Vec<PackIssue> {
    // Peel nodes that cannot be on a cycle (no incoming or no outgoing edges) until stable.
    let mut alive: BTreeSet<&str> = requires.keys().map(String::as_str).collect();
    loop {
        let mut indeg: BTreeMap<&str, usize> = alive.iter().map(|k| (*k, 0)).collect();
        let mut outdeg: BTreeMap<&str, usize> = alive.iter().map(|k| (*k, 0)).collect();
        for k in &alive {
            for d in &requires[*k] {
                if alive.contains(d.as_str()) {
                    *indeg.get_mut(d.as_str()).unwrap() += 1;
                    *outdeg.get_mut(*k).unwrap() += 1;
                }
            }
        }
        let dead: Vec<&str> = alive
            .iter()
            .copied()
            .filter(|k| indeg[k] == 0 || outdeg[k] == 0)
            .collect();
        if dead.is_empty() {
            break;
        }
        for k in dead {
            alive.remove(k);
        }
    }
    // Group what is left by mutual reachability.
    let reach = |from: &str| -> BTreeSet<&str> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![from];
        while let Some(n) = stack.pop() {
            for d in &requires[n] {
                if let Some(k) = alive.get(d.as_str()) {
                    if seen.insert(*k) {
                        stack.push(*k);
                    }
                }
            }
        }
        seen
    };
    let reaches: BTreeMap<&str, BTreeSet<&str>> = alive.iter().map(|k| (*k, reach(k))).collect();
    let mut done: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for k in &alive {
        if done.contains(k) {
            continue;
        }
        let group: Vec<String> = alive
            .iter()
            .filter(|o| (*o == k) || (reaches[k].contains(**o) && reaches[**o].contains(k)))
            .map(|o| (*o).to_string())
            .collect();
        for o in &group {
            done.insert(alive.get(o.as_str()).unwrap());
        }
        if group.len() > 1 || reaches[k].contains(k) {
            out.push(PackIssue::Cycle { mods: group });
        }
    }
    out
}
