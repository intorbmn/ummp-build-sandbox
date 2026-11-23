//! Application: migration engine (Phase 6 MVP).
//!
//! Pipeline slice implemented here: **Plan → Transform → Review (diff/report)
//! → Apply (with checkpoint) → Rollback**.
//!
//! - The planner selects API mappings from the knowledge base for the target
//!   loader whose version lies in `(source, target]` and groups them into
//!   ordered *stages*, one per Minecraft version. Stages run in order so a
//!   chain `A→B` (1.20) then `B→C` (1.21) ends at `C`.
//! - Source transformers implement [`SourceTransformer`]. The MVP ships
//!   [`TokenRenameTransformer`] for Java/Kotlin (ADR 0007).
//! - Edits are applied only after a checkpoint is taken; any write failure
//!   restores the checkpoint (ADR 0005, ADR 0006).

use crate::adapter::{AdapterContext, AdapterRegistry};
use crate::build_script::{migrate_build_file, required_java, BuildFile};
use crate::checkpoint::{collect_files, CheckpointError, CheckpointStore};
use crate::descriptor::{migrate_descriptor, DescriptorKind, DescriptorTarget};
use crate::filesystem::{resolve_within, FsError};
use crate::knowledge::{BreakingChange, KnowledgeBase, Mapping};
use crate::mixin::{
    count_method_targets, is_mixin_class, is_mixin_config, migrate_mixin_config,
    migrate_mixin_members, migrate_mixin_strings,
};
use crate::persistence::DbError;
use crate::project::Project;
use crate::resources::{datapack_move, is_pack_mcmeta, migrate_pack_mcmeta, FileMove, PackFormats};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

/// Files larger than this are reported and skipped, never transformed.
pub const MAX_SOURCE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("knowledge base: {0}")]
    Db(#[from] DbError),
    #[error("fs: {0}")]
    Fs(#[from] FsError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("checkpoint: {0}")]
    Checkpoint(#[from] CheckpointError),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("apply failed on {path} ({cause}); rolled back: {rolled_back}")]
    ApplyFailed {
        path: PathBuf,
        cause: String,
        rolled_back: bool,
    },
}

// ---------------------------------------------------------------------------
// Request / plan
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRequest {
    pub source_minecraft: String,
    pub target_minecraft: String,
    pub target_loader: String,
    /// Mappings below this confidence are reported, not applied.
    pub min_confidence: f64,
    /// Loader the mod uses today; when it differs from `target_loader` the loader
    /// converter runs (Phase 9). `None` = same loader.
    #[serde(default)]
    pub source_loader: Option<String>,
}

/// One API rename taken from a knowledge-base [`Mapping`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenameRule {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub confidence: f64,
    pub version: String,
}

impl RenameRule {
    /// `true` for dotted names such as `net.minecraft.block.Block`.
    pub fn is_qualified(&self) -> bool {
        self.from.contains('.')
    }
}

/// All rules introduced by one Minecraft version, applied together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub version: String,
    pub rules: Vec<RenameRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedMapping {
    pub mapping: Mapping,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub request: MigrationRequest,
    pub stages: Vec<Stage>,
    pub skipped: Vec<SkippedMapping>,
    pub breaking_changes: Vec<BreakingChange>,
    pub recipes: Vec<RecipeOutcome>,
    /// Versions the mod descriptors should declare (Phase 6.B).
    pub descriptor_target: DescriptorTarget,
    /// Minimum Java release for the target (Phase 6.C/6.E).
    pub java: Option<u32>,
    /// Pack formats for the target, from the KB (Phase 6.D).
    pub pack_formats: PackFormats,
    /// Source < 1.21 <= target: move data pack folders to singular names.
    pub datapack_renames: bool,
}

/// One step of a knowledge-base recipe (`Recipe.steps`), e.g.
/// `{"op": "rename", "from": "a.B", "to": "c.B"}` or `{"op": "warn", "message": "..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RecipeStep {
    Rename { from: String, to: String },
    Warn { message: String },
}

/// What happened to a recipe during planning (Phase 6.G).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeOutcome {
    pub recipe_id: String,
    pub executed: bool,
    pub note: String,
}

impl MigrationPlan {
    pub fn rule_count(&self) -> usize {
        self.stages.iter().map(|s| s.rules.len()).sum()
    }
}

fn parse_version(v: &str) -> Option<Vec<u32>> {
    v.split('.').map(|p| p.parse().ok()).collect()
}

/// Compare dotted numeric versions, padding with zeros (`1.21` == `1.21.0`).
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    let (mut a, mut b) = (parse_version(a)?, parse_version(b)?);
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    Some(a.cmp(&b))
}

/// `true` when `v` is in `(source, target]`.
fn in_upgrade_range(v: &str, source: &str, target: &str) -> bool {
    matches!(compare_versions(v, source), Some(Ordering::Greater))
        && matches!(
            compare_versions(v, target),
            Some(Ordering::Less | Ordering::Equal)
        )
}

pub struct MigrationPlanner;

impl MigrationPlanner {
    pub fn plan(
        kb: &KnowledgeBase,
        request: &MigrationRequest,
    ) -> Result<MigrationPlan, MigrationError> {
        let (src, tgt) = (&request.source_minecraft, &request.target_minecraft);
        match compare_versions(src, tgt) {
            None => {
                return Err(MigrationError::InvalidRequest(format!(
                    "versions must be numeric (got '{src}' -> '{tgt}')"
                )))
            }
            Some(Ordering::Greater) => {
                return Err(MigrationError::InvalidRequest(format!(
                    "downgrades are not supported ('{src}' -> '{tgt}')"
                )))
            }
            _ => {}
        }

        let mut skipped = Vec::new();
        // version -> from -> rule
        let mut by_version: BTreeMap<Vec<u32>, (String, BTreeMap<String, RenameRule>)> =
            BTreeMap::new();
        let mut conflicts: Vec<(Vec<u32>, String)> = Vec::new();

        for m in kb.mappings_by_loader(&request.target_loader)? {
            if !in_upgrade_range(&m.version, src, tgt) {
                continue;
            }
            if m.confidence < request.min_confidence {
                skipped.push(SkippedMapping {
                    reason: format!(
                        "confidence {:.2} below threshold {:.2}",
                        m.confidence, request.min_confidence
                    ),
                    mapping: m,
                });
                continue;
            }
            if m.from_api == m.to_api || m.from_api.is_empty() || m.to_api.is_empty() {
                skipped.push(SkippedMapping {
                    reason: "no-op or empty mapping".into(),
                    mapping: m,
                });
                continue;
            }
            let key = parse_version(&m.version).unwrap_or_default();
            let entry = by_version
                .entry(key.clone())
                .or_insert_with(|| (m.version.clone(), BTreeMap::new()));
            match entry.1.get(&m.from_api) {
                Some(existing) if existing.to != m.to_api => {
                    conflicts.push((key, m.from_api.clone()));
                    skipped.push(SkippedMapping {
                        reason: format!(
                            "conflicts with '{}' -> '{}' for the same version",
                            existing.from, existing.to
                        ),
                        mapping: m,
                    });
                }
                Some(_) => {} // duplicate of an identical rule
                None => {
                    entry.1.insert(
                        m.from_api.clone(),
                        RenameRule {
                            from: m.from_api.clone(),
                            to: m.to_api.clone(),
                            kind: m.kind.clone(),
                            confidence: m.confidence,
                            version: m.version.clone(),
                        },
                    );
                }
            }
        }
        // A conflicting name is ambiguous: drop every rule for it in that version.
        for (key, from) in conflicts {
            if let Some((_, rules)) = by_version.get_mut(&key) {
                if let Some(rule) = rules.remove(&from) {
                    skipped.push(SkippedMapping {
                        reason: "ambiguous: several targets for the same name".into(),
                        mapping: Mapping {
                            version: rule.version,
                            loader: request.target_loader.clone(),
                            from_api: rule.from,
                            to_api: rule.to,
                            kind: rule.kind,
                            source: "planner".into(),
                            confidence: rule.confidence,
                        },
                    });
                }
            }
        }

        // Phase 6.G: tested recipes contribute rename rules to their version's
        // stage; untested or unparsable recipes are reported, never executed.
        let mut recipes = Vec::new();
        for recipe in kb.recipes_by_loader(&request.target_loader)? {
            if !in_upgrade_range(&recipe.version, src, tgt) {
                continue;
            }
            let outcome = |executed: bool, note: String| RecipeOutcome {
                recipe_id: recipe.recipe_id.clone(),
                executed,
                note,
            };
            if !recipe.tested {
                recipes.push(outcome(false, "not marked tested".into()));
                continue;
            }
            let steps: Result<Vec<RecipeStep>, _> = recipe
                .steps
                .iter()
                .map(|v| serde_json::from_value(v.clone()))
                .collect();
            let Ok(steps) = steps else {
                recipes.push(outcome(false, "unsupported step format".into()));
                continue;
            };
            let key = parse_version(&recipe.version).unwrap_or_default();
            let mut notes = Vec::new();
            for step in steps {
                match step {
                    RecipeStep::Rename { from, to } => {
                        let rules = &mut by_version
                            .entry(key.clone())
                            .or_insert_with(|| (recipe.version.clone(), BTreeMap::new()))
                            .1;
                        match rules.get(&from) {
                            Some(existing) if existing.to != to => notes.push(format!(
                                "rename {from} skipped: KB maps it to {}",
                                existing.to
                            )),
                            Some(_) => {}
                            None => {
                                rules.insert(
                                    from.clone(),
                                    RenameRule {
                                        from,
                                        to,
                                        kind: format!("recipe:{}", recipe.recipe_id),
                                        confidence: 1.0,
                                        version: recipe.version.clone(),
                                    },
                                );
                            }
                        }
                    }
                    RecipeStep::Warn { message } => notes.push(message),
                }
            }
            let note = if notes.is_empty() {
                recipe.description.clone()
            } else {
                notes.join("; ")
            };
            recipes.push(outcome(true, note));
        }

        let mut stages: Vec<Stage> = by_version
            .into_values()
            .filter(|(_, rules)| !rules.is_empty())
            .map(|(version, rules)| Stage {
                version,
                rules: rules.into_values().collect(),
            })
            .collect();

        // Loader conversion (Phase 9): KB rows with loader "<from>-><to>" form one last stage.
        if let Some(from) = request
            .source_loader
            .as_deref()
            .filter(|f| *f != request.target_loader)
        {
            let pair = format!("{from}->{}", request.target_loader);
            let rules: Vec<RenameRule> = kb
                .mappings_by_loader(&pair)?
                .into_iter()
                .filter(|m| m.from_api != m.to_api)
                .filter(|m| m.confidence >= request.min_confidence)
                .map(|m| RenameRule {
                    from: m.from_api,
                    to: m.to_api,
                    kind: m.kind,
                    confidence: m.confidence,
                    version: pair.clone(),
                })
                .collect();
            if !rules.is_empty() {
                stages.push(Stage {
                    version: pair,
                    rules,
                });
            }
        }

        // `search_breaking_changes("")` matches every row (LIKE '%%').
        let breaking_changes = kb
            .search_breaking_changes("")?
            .into_iter()
            .filter(|b| in_upgrade_range(&b.version, src, tgt))
            .collect();
        let pack_format = |kind: &str| -> Result<Option<u32>, MigrationError> {
            Ok(kb
                .pack_formats(kind)?
                .into_iter()
                .find(|(mc, _)| compare_versions(mc, tgt) == Some(Ordering::Equal))
                .map(|(_, f)| f))
        };
        let pack_formats = PackFormats {
            resource: pack_format("resource")?,
            data: pack_format("data")?,
        };

        // Loader version for the exact target Minecraft version; highest wins.
        let loader_version = kb
            .compatibilities_by_loader(&request.target_loader)?
            .into_iter()
            .filter(|c| compare_versions(&c.minecraft_version, tgt) == Some(Ordering::Equal))
            .map(|c| c.loader_version)
            .max_by(|a, b| compare_versions(a, b).unwrap_or_else(|| a.cmp(b)));

        Ok(MigrationPlan {
            request: request.clone(),
            stages,
            skipped,
            breaking_changes,
            recipes,
            descriptor_target: DescriptorTarget {
                minecraft: tgt.clone(),
                loader: request.target_loader.clone(),
                loader_version,
            },
            java: required_java(tgt),
            pack_formats,
            datapack_renames: in_upgrade_range("1.21", src, tgt),
        })
    }
}

// ---------------------------------------------------------------------------
// Source transformation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    pub line: usize,
    pub from: String,
    pub to: String,
}

/// Where a candidate match sits, used to decide whether to rewrite it.
#[derive(Debug, Clone, Copy)]
pub struct MatchContext<'a> {
    pub rule: &'a RenameRule,
    /// The matched name follows a `.` (e.g. `foo.Block`, `a.b.Block`).
    pub preceded_by_dot: bool,
    /// The name is part of an `import` / `import static` statement.
    pub in_import: bool,
    /// The dotted path before the matched name (`com.example` in
    /// `com.example.DemoWidget`); empty when nothing precedes it.
    pub qualifier: &'a str,
}

/// Rewrite policy for a token that matches a rule (ADR 0008).
///
/// Qualified rules and import statements name a type by its full path, so
/// they always match. A *simple-name* rule matches a standalone name, and a
/// name after a dot only when everything before it is lowercase, i.e. looks
/// like a package path (`com.example.DemoWidget`). `Outer.DemoWidget` (a
/// member or nested type) and `foo().DemoWidget` are left alone. Without type
/// information a lowercase variable (`x.DemoWidget`) is indistinguishable from
/// a package and is renamed too; this is the accepted trade-off.
pub fn should_rewrite(ctx: &MatchContext<'_>) -> bool {
    if ctx.rule.is_qualified() || ctx.in_import || !ctx.preceded_by_dot {
        return true;
    }
    !ctx.qualifier.is_empty()
        && ctx
            .qualifier
            .split('.')
            .all(|seg| seg.starts_with(|c: char| c.is_ascii_lowercase()))
}

pub trait SourceTransformer: Send + Sync {
    fn name(&self) -> &'static str;
    fn applies_to(&self, path: &Path) -> bool;
    /// Apply one stage of rules. Must not change anything else in `content`.
    fn transform(&self, content: &str, rules: &[RenameRule]) -> (String, Vec<Replacement>);
}

/// Token-aware renamer for Java and Kotlin sources (ADR 0007).
///
/// Matches whole identifiers or dotted paths only, never inside a longer
/// identifier, and never inside comments, string, char or text-block literals.
pub struct TokenRenameTransformer;

const fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b >= 0x80
}

const fn is_ident_cont(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

fn find_seq(b: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    b.get(from..)?
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|p| p + from)
}

/// Skip a `"…"` or `'…'` literal starting at `i`; stops at an unescaped
/// closing quote or at end of line (unterminated literal).
const fn skip_quoted(b: &[u8], i: usize, quote: u8) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'\n' => return j,
            c if c == quote => return j + 1,
            _ => j += 1,
        }
    }
    b.len()
}

impl SourceTransformer for TokenRenameTransformer {
    fn name(&self) -> &'static str {
        "token-rename"
    }

    /// Java and Kotlin sources. Build scripts (`*.gradle.kts`) are excluded:
    /// they belong to the not-yet-implemented build migration step.
    fn applies_to(&self, path: &Path) -> bool {
        matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("java" | "kt" | "groovy" | "scala")
        )
    }

    fn transform(&self, src: &str, rules: &[RenameRule]) -> (String, Vec<Replacement>) {
        let bytes = src.as_bytes();
        let src_len = bytes.len();
        let rule_segs: Vec<(Vec<&str>, &RenameRule)> = rules
            .iter()
            .map(|r| (r.from.split('.').collect(), r))
            .collect();

        let mut out = String::with_capacity(src_len);
        let mut reps = Vec::new();
        let mut copied = 0; // bytes of `src` already copied to `out`
        let (mut line, mut line_pos) = (1, 0); // incremental line counter
        let mut prev_words: [Option<&str>; 2] = [None, None];
        let mut i = 0;

        while i < src_len {
            let c = bytes[i];
            if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
                i = find_seq(bytes, i, b"\n").unwrap_or(src_len);
                prev_words = [None, None];
                continue;
            }
            if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
                i = find_seq(bytes, i + 2, b"*/").map_or(src_len, |p| p + 2);
                continue;
            }
            if c == b'"' {
                i = if bytes[i..].starts_with(b"\"\"\"") {
                    find_seq(bytes, i + 3, b"\"\"\"").map_or(src_len, |p| p + 3)
                } else {
                    skip_quoted(bytes, i, b'"')
                };
                prev_words = [None, None];
                continue;
            }
            if c == b'\'' {
                // Groovy triple-single-quoted strings span lines.
                i = if bytes[i..].starts_with(b"'''") {
                    find_seq(bytes, i + 3, b"'''").map_or(src_len, |p| p + 3)
                } else {
                    skip_quoted(bytes, i, b'\'')
                };
                prev_words = [None, None];
                continue;
            }
            if c.is_ascii_digit() {
                // numeric literal such as 0xFF or 1.5f: never an identifier
                while i < src_len && is_ident_cont(bytes[i]) {
                    i += 1;
                }
                continue;
            }
            if !is_ident_start(c) {
                if !c.is_ascii_whitespace() {
                    prev_words = [None, None];
                }
                i += 1;
                continue;
            }

            // Dotted path: ident ( '.' ident )*
            let mut segs: Vec<(usize, usize)> = Vec::new();
            let mut j = i;
            loop {
                let s = j;
                while j < src_len && is_ident_cont(bytes[j]) {
                    j += 1;
                }
                segs.push((s, j));
                if j + 1 < src_len && bytes[j] == b'.' && is_ident_start(bytes[j + 1]) {
                    j += 1;
                } else {
                    break;
                }
            }

            let in_import = prev_words[0] == Some("import")
                || (prev_words[0] == Some("static") && prev_words[1] == Some("import"));
            let lead_dot = i > 0 && bytes[i - 1] == b'.';

            let mut k = 0;
            while k < segs.len() {
                let mut best: Option<(usize, &RenameRule)> = None;
                for (rs, rule) in &rule_segs {
                    let len = rs.len();
                    if k + len > segs.len() || best.is_some_and(|(bl, _)| bl >= len) {
                        continue;
                    }
                    let matches = rs
                        .iter()
                        .enumerate()
                        .all(|(t, seg)| &src[segs[k + t].0..segs[k + t].1] == *seg);
                    let qualifier = if k > 0 {
                        &src[segs[0].0..segs[k - 1].1]
                    } else {
                        ""
                    };
                    let ctx = MatchContext {
                        rule,
                        preceded_by_dot: k > 0 || lead_dot,
                        in_import,
                        qualifier,
                    };
                    if matches && should_rewrite(&ctx) {
                        best = Some((len, rule));
                    }
                }
                if let Some((len, rule)) = best {
                    let (start, end) = (segs[k].0, segs[k + len - 1].1);
                    line += src[line_pos..start].bytes().filter(|&x| x == b'\n').count();
                    line_pos = start;
                    out.push_str(&src[copied..start]);
                    out.push_str(&rule.to);
                    copied = end;
                    reps.push(Replacement {
                        line,
                        from: rule.from.clone(),
                        to: rule.to.clone(),
                    });
                    k += len;
                } else {
                    k += 1;
                }
            }

            prev_words = [Some(&src[i..j]), prev_words[0]];
            i = j;
        }
        out.push_str(&src[copied..]);
        (out, reps)
    }
}

// ---------------------------------------------------------------------------
// Edits, diff, report
// ---------------------------------------------------------------------------

/// A proposed change to one file. Both rule-based transforms and (later) AI
/// agents produce `FileEdit`s; only the engine applies them (ADR 0005).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEdit {
    /// Path relative to the project root.
    pub path: PathBuf,
    pub original: String,
    pub updated: String,
    pub replacements: Vec<Replacement>,
}

impl FileEdit {
    pub fn unified_diff(&self) -> String {
        let name = self.path.to_string_lossy().replace('\\', "/");
        similar::TextDiff::from_lines(&self.original, &self.updated)
            .unified_diff()
            .context_radius(3)
            .header(&format!("a/{name}"), &format!("b/{name}"))
            .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeSummary {
    pub path: PathBuf,
    pub replacements: Vec<Replacement>,
    pub diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    pub request: MigrationRequest,
    pub dry_run: bool,
    pub checkpoint_id: Option<Uuid>,
    pub files_scanned: usize,
    pub changes: Vec<FileChangeSummary>,
    /// Files moved (1.21 data pack folder renames).
    pub moves: Vec<FileMove>,
    pub skipped_mappings: Vec<SkippedMapping>,
    pub breaking_changes: Vec<BreakingChange>,
    pub recipes: Vec<RecipeOutcome>,
    pub warnings: Vec<String>,
    /// Files removed (e.g. the old descriptor after a loader conversion).
    #[serde(default)]
    pub deletes: Vec<PathBuf>,
    /// Technology findings from the adapters (Phase 9).
    #[serde(default)]
    pub technologies: Vec<crate::adapter::TechFinding>,
}

impl MigrationReport {
    pub fn total_replacements(&self) -> usize {
        self.changes.iter().map(|c| c.replacements.len()).sum()
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_markdown(&self) -> String {
        let r = &self.request;
        let mut out = String::new();
        let _ = writeln!(out, "# Migration Report\n");
        let _ = writeln!(
            out,
            "- Target: Minecraft {} -> {} on `{}`",
            r.source_minecraft, r.target_minecraft, r.target_loader
        );
        let mode = if self.dry_run {
            "dry run (nothing written)"
        } else {
            "applied"
        };
        let _ = writeln!(out, "- Mode: {mode}");
        if let Some(id) = self.checkpoint_id {
            let _ = writeln!(out, "- Checkpoint: `{id}` (use it to roll back)");
        }
        let _ = writeln!(
            out,
            "- Files scanned: {}, files changed: {}, files moved: {}, replacements: {}\n",
            self.files_scanned,
            self.changes.len(),
            self.moves.len(),
            self.total_replacements()
        );

        if !self.breaking_changes.is_empty() {
            let _ = writeln!(out, "## Breaking changes to review manually\n");
            for b in &self.breaking_changes {
                let _ = writeln!(out, "- **{}** [{}] {}", b.version, b.severity, b.change);
                if let Some(w) = &b.workaround {
                    let _ = writeln!(out, "  - Workaround: {w}");
                }
            }
            out.push('\n');
        }
        if !self.recipes.is_empty() {
            let _ = writeln!(out, "## Recipes\n");
            for o in &self.recipes {
                let state = if o.executed {
                    "executed"
                } else {
                    "not executed"
                };
                let _ = writeln!(out, "- `{}` ({state}): {}", o.recipe_id, o.note);
            }
            out.push('\n');
        }
        if !self.skipped_mappings.is_empty() {
            let _ = writeln!(out, "## Mappings not applied\n");
            for s in &self.skipped_mappings {
                let _ = writeln!(
                    out,
                    "- `{}` -> `{}` ({}): {}",
                    s.mapping.from_api, s.mapping.to_api, s.mapping.version, s.reason
                );
            }
            out.push('\n');
        }
        if !self.technologies.is_empty() {
            let _ = writeln!(out, "## Technologies\n");
            out.push_str(&crate::adapter::findings_text(&self.technologies));
            out.push('\n');
        }
        if !self.deletes.is_empty() {
            let _ = writeln!(out, "## Deleted files\n");
            for d in &self.deletes {
                let _ = writeln!(out, "- `{}`", d.display());
            }
            out.push('\n');
        }
        if !self.warnings.is_empty() {
            let _ = writeln!(out, "## Warnings\n");
            for w in &self.warnings {
                let _ = writeln!(out, "- {w}");
            }
            out.push('\n');
        }
        if !self.moves.is_empty() {
            let _ = writeln!(out, "## Moved files\n");
            for m in &self.moves {
                let _ = writeln!(out, "- `{}` -> `{}`", m.from.display(), m.to.display());
            }
            out.push('\n');
        }
        if !self.changes.is_empty() {
            let _ = writeln!(out, "## Diff\n");
            for c in &self.changes {
                let _ = writeln!(out, "```diff\n{}```\n", c.diff);
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

pub struct RunOptions<'a> {
    pub dry_run: bool,
    /// Where checkpoints are stored; must be outside the project.
    pub snapshot_root: &'a Path,
    pub session_id: Uuid,
}

/// Everything a run would change, computed without writing.
#[derive(Debug, Clone, Default)]
pub struct EditSet {
    pub edits: Vec<FileEdit>,
    pub moves: Vec<FileMove>,
    /// Files to delete (relative paths), e.g. a descriptor replaced by loader conversion.
    pub deletes: Vec<PathBuf>,
    /// Technology findings from the adapters (Phase 9).
    pub technologies: Vec<crate::adapter::TechFinding>,
    pub files_scanned: usize,
    pub warnings: Vec<String>,
}

pub struct MigrationEngine {
    transformers: Vec<Box<dyn SourceTransformer>>,
    adapters: AdapterRegistry,
}

impl Default for MigrationEngine {
    fn default() -> Self {
        Self {
            transformers: vec![Box::new(TokenRenameTransformer)],
            adapters: AdapterRegistry::default(),
        }
    }
}

/// How a file is migrated.
enum Handler<'a> {
    Descriptor(DescriptorKind),
    Build(BuildFile),
    MixinConfig,
    PackMeta,
    Source(&'a dyn SourceTransformer),
}

impl MigrationEngine {
    pub fn new(transformers: Vec<Box<dyn SourceTransformer>>) -> Self {
        Self {
            transformers,
            adapters: AdapterRegistry::default(),
        }
    }

    /// Replaces the technology adapters (tests, plugins).
    #[must_use]
    pub fn with_adapters(mut self, adapters: AdapterRegistry) -> Self {
        self.adapters = adapters;
        self
    }

    fn handler(&self, rel: &Path) -> Option<Handler<'_>> {
        if let Some(k) = DescriptorKind::from_path(rel) {
            return Some(Handler::Descriptor(k));
        }
        if let Some(b) = BuildFile::from_path(rel) {
            return Some(Handler::Build(b));
        }
        if is_mixin_config(rel) {
            return Some(Handler::MixinConfig);
        }
        if is_pack_mcmeta(rel) {
            return Some(Handler::PackMeta);
        }
        self.transformers
            .iter()
            .find(|t| t.applies_to(rel))
            .map(|t| Handler::Source(t.as_ref()))
    }

    /// Compute edits and moves without writing anything.
    pub fn compute_edits(
        &self,
        plan: &MigrationPlan,
        project: &Project,
    ) -> Result<EditSet, MigrationError> {
        let mut set = EditSet::default();
        let files = collect_files(&project.root)?;
        let cx = AdapterContext {
            root: &project.root,
            files: &files,
            plan: Some(plan),
            source_loader: plan.request.source_loader.clone(),
        };

        if plan.datapack_renames {
            let existing: std::collections::HashSet<&PathBuf> = files.iter().collect();
            for rel in &files {
                if let Some(to) = datapack_move(rel) {
                    if existing.contains(&to) {
                        set.warnings.push(format!(
                            "{}: not moved, {} already exists",
                            rel.display(),
                            to.display()
                        ));
                    } else {
                        set.moves.push(FileMove {
                            from: rel.clone(),
                            to,
                        });
                    }
                }
            }
        }

        for rel in files.iter().cloned() {
            if self.adapters.owns(&cx, &rel) {
                continue;
            }
            let Some(handler) = self.handler(&rel) else {
                continue;
            };
            let abs = resolve_within(&project.root, &rel)?;
            if std::fs::metadata(&abs)?.len() > MAX_SOURCE_BYTES {
                set.warnings.push(format!(
                    "{}: larger than {MAX_SOURCE_BYTES} bytes, skipped",
                    rel.display()
                ));
                continue;
            }
            set.files_scanned += 1;
            let Ok(original) = String::from_utf8(project.fs.read(&abs)?) else {
                set.warnings
                    .push(format!("{}: not valid UTF-8, skipped", rel.display()));
                continue;
            };

            let mut current = original.clone();
            let mut replacements = Vec::new();
            let mut notes = Vec::new();
            let mut take = |r: crate::descriptor::EditResult,
                            current: &mut String,
                            replacements: &mut Vec<Replacement>| {
                *current = r.updated;
                replacements.extend(r.replacements);
                notes.extend(r.warnings);
            };
            match handler {
                Handler::Descriptor(kind) => {
                    let r = migrate_descriptor(kind, &original, &plan.descriptor_target);
                    take(r, &mut current, &mut replacements);
                }
                Handler::Build(kind) => {
                    let r = migrate_build_file(kind, &original, &plan.descriptor_target, plan.java);
                    take(r, &mut current, &mut replacements);
                }
                Handler::MixinConfig => {
                    take(
                        migrate_mixin_config(&original, plan.java),
                        &mut current,
                        &mut replacements,
                    );
                }
                Handler::PackMeta => {
                    let dir = rel.parent().unwrap_or_else(|| Path::new(""));
                    let has = |d: &str| project.root.join(dir).join(d).is_dir();
                    let data_only = has("data") && !has("assets");
                    take(
                        migrate_pack_mcmeta(&original, plan.pack_formats, data_only),
                        &mut current,
                        &mut replacements,
                    );
                }
                Handler::Source(tf) => {
                    let mixin = is_mixin_class(&original);
                    for stage in &plan.stages {
                        let (next, reps) = tf.transform(&current, &stage.rules);
                        current = next;
                        replacements.extend(reps);
                        if mixin {
                            let r = migrate_mixin_strings(&current, &stage.rules);
                            take(r, &mut current, &mut replacements);
                            let r = migrate_mixin_members(&current, &stage.rules);
                            take(r, &mut current, &mut replacements);
                        }
                    }
                    current = self.adapters.rewrite(&cx, &rel, &current);
                    if mixin {
                        let n = count_method_targets(&original);
                        if n > 0 {
                            notes.push(format!(
                                "{n} injector method target(s) not verified (no method mappings in the knowledge base)"
                            ));
                        }
                    }
                }
            }
            set.warnings
                .extend(notes.into_iter().map(|w| format!("{}: {w}", rel.display())));
            if current != original {
                set.edits.push(FileEdit {
                    path: rel,
                    original,
                    updated: current,
                    replacements,
                });
            }
        }
        for adapter in self.adapters.adapters() {
            let extra = adapter.edits(&cx)?;
            for e in extra.edits {
                // An adapter edit replaces any generic edit of the same file.
                set.edits.retain(|x| x.path != e.path);
                set.edits.push(e);
            }
            set.moves.extend(extra.moves);
            set.deletes.extend(extra.deletes);
            set.warnings.extend(extra.warnings);
        }
        set.technologies = self.adapters.findings(&cx);
        Ok(set)
    }

    fn apply(
        set: &EditSet,
        project: &Project,
    ) -> Result<(), (PathBuf, crate::filesystem::FsError)> {
        for edit in &set.edits {
            resolve_within(&project.root, &edit.path)
                .and_then(|abs| project.fs.write(&abs, edit.updated.as_bytes()))
                .map_err(|e| (edit.path.clone(), e))?;
        }
        for m in &set.moves {
            let moved = (|| {
                let from = resolve_within(&project.root, &m.from)?;
                let to = resolve_within(&project.root, &m.to)?;
                let data = project.fs.read(&from)?;
                project.fs.write(&to, &data)?;
                project.fs.remove(&from)?;
                // Best effort: drop now-empty source directories inside the project.
                let mut dir = from.parent();
                while let Some(d) = dir.filter(|d| *d != project.root) {
                    if std::fs::remove_dir(d).is_err() {
                        break;
                    }
                    dir = d.parent();
                }
                Ok(())
            })();
            moved.map_err(|e| (m.from.clone(), e))?;
        }
        for path in &set.deletes {
            resolve_within(&project.root, path)
                .and_then(|abs| project.fs.remove(&abs))
                .map_err(|e| (path.clone(), e))?;
        }
        Ok(())
    }

    /// Apply an edit set (from the rules or from approved AI proposals):
    /// checkpoint first, write, and restore the checkpoint if anything fails.
    /// Returns the checkpoint id, or `None` when there was nothing to apply.
    pub fn apply_edit_set(
        &self,
        set: &EditSet,
        project: &Project,
        store: &mut CheckpointStore,
        opts: &RunOptions<'_>,
    ) -> Result<Option<Uuid>, MigrationError> {
        if set.edits.is_empty() && set.moves.is_empty() && set.deletes.is_empty() {
            return Ok(None);
        }
        let cp = store.snapshot(
            opts.session_id,
            "pre-transform",
            &project.root,
            opts.snapshot_root,
        )?;
        if let Err((path, e)) = Self::apply(set, project) {
            let rolled_back = store.restore(cp.id, &project.root).is_ok();
            return Err(MigrationError::ApplyFailed {
                path,
                cause: e.to_string(),
                rolled_back,
            });
        }
        Ok(Some(cp.id))
    }

    /// Test-only fault injection: persist the pre-transform checkpoint, apply a
    /// prefix of edits, then return without rollback to model abrupt process death.
    #[cfg(test)]
    fn apply_with_interruption(
        set: &EditSet,
        project: &Project,
        store: &mut CheckpointStore,
        opts: &RunOptions<'_>,
        edits_before_interruption: usize,
    ) -> Result<Uuid, MigrationError> {
        let checkpoint = store.snapshot(
            opts.session_id,
            "pre-transform",
            &project.root,
            opts.snapshot_root,
        )?;
        for edit in set.edits.iter().take(edits_before_interruption) {
            let path = resolve_within(&project.root, &edit.path)?;
            project.fs.write(&path, edit.updated.as_bytes())?;
        }
        Ok(checkpoint.id)
    }

    /// Run Transform (+ Apply unless `dry_run`). On apply, a checkpoint is
    /// taken first; if any write or move fails the project is restored from it.
    pub fn run(
        &self,
        plan: &MigrationPlan,
        project: &Project,
        store: &mut CheckpointStore,
        opts: &RunOptions<'_>,
    ) -> Result<MigrationReport, MigrationError> {
        let set = self.compute_edits(plan, project)?;
        let checkpoint_id = if opts.dry_run {
            None
        } else {
            self.apply_edit_set(&set, project, store, opts)?
        };

        Ok(MigrationReport {
            request: plan.request.clone(),
            dry_run: opts.dry_run,
            checkpoint_id,
            files_scanned: set.files_scanned,
            changes: set
                .edits
                .iter()
                .map(|e| FileChangeSummary {
                    path: e.path.clone(),
                    replacements: e.replacements.clone(),
                    diff: e.unified_diff(),
                })
                .collect(),
            moves: set.moves,
            skipped_mappings: plan.skipped.clone(),
            breaking_changes: plan.breaking_changes.clone(),
            recipes: plan.recipes.clone(),
            warnings: set.warnings,
            deletes: set.deletes,
            technologies: set.technologies,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(from: &str, to: &str) -> RenameRule {
        RenameRule {
            from: from.into(),
            to: to.into(),
            kind: "rename".into(),
            confidence: 1.0,
            version: "1.21".into(),
        }
    }

    fn rewrite(src: &str, rules: &[RenameRule]) -> String {
        TokenRenameTransformer.transform(src, rules).0
    }

    #[test]
    fn interrupted_apply_recovers_original_tree_from_durable_manifest() {
        let project_dir = tempfile::tempdir().unwrap();
        let snapshots_dir = tempfile::tempdir().unwrap();
        let first = project_dir.path().join("src/First.java");
        let second = project_dir.path().join("src/Second.java");
        std::fs::create_dir_all(first.parent().unwrap()).unwrap();
        std::fs::write(&first, "class First {}").unwrap();
        std::fs::write(&second, "class Second {}").unwrap();

        let project = Project::new(project_dir.path().to_path_buf());
        let session_id = Uuid::new_v4();
        let opts = RunOptions {
            dry_run: false,
            snapshot_root: snapshots_dir.path(),
            session_id,
        };
        let set = EditSet {
            edits: vec![
                FileEdit {
                    path: PathBuf::from("src/First.java"),
                    original: "class First {}".into(),
                    updated: "class FirstChanged {}".into(),
                    replacements: Vec::new(),
                },
                FileEdit {
                    path: PathBuf::from("src/Second.java"),
                    original: "class Second {}".into(),
                    updated: "class SecondChanged {}".into(),
                    replacements: Vec::new(),
                },
            ],
            ..EditSet::default()
        };

        let mut store = CheckpointStore::new();
        let checkpoint_id =
            MigrationEngine::apply_with_interruption(&set, &project, &mut store, &opts, 1).unwrap();

        // One edit reached disk; the next process reloads only durable metadata.
        assert_eq!(
            std::fs::read_to_string(&first).unwrap(),
            "class FirstChanged {}"
        );
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "class Second {}");
        let recovered_store: CheckpointStore = serde_json::from_slice(
            &std::fs::read(snapshots_dir.path().join("checkpoints.json")).unwrap(),
        )
        .unwrap();
        let checkpoint = recovered_store
            .checkpoints
            .iter()
            .find(|item| item.session_id == session_id)
            .unwrap();
        assert_eq!(checkpoint.id, checkpoint_id);

        recovered_store
            .restore(checkpoint_id, &project.root)
            .unwrap();
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "class First {}");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "class Second {}");
    }

    #[test]
    fn test_compare_versions() {
        assert_eq!(compare_versions("1.21", "1.21.0"), Some(Ordering::Equal));
        assert_eq!(compare_versions("1.20.1", "1.21"), Some(Ordering::Less));
        assert_eq!(compare_versions("1.9", "1.10"), Some(Ordering::Less));
        assert_eq!(compare_versions("1.21-pre1", "1.21"), None);
        assert!(in_upgrade_range("1.21", "1.20.1", "1.21"));
        assert!(!in_upgrade_range("1.20.1", "1.20.1", "1.21"));
    }

    #[test]
    fn test_whole_identifier_only() {
        let out = rewrite(
            "DemoWidget b = new DemoWidget(); DemoWidgetEntity e; MyDemoWidget m;",
            &[rule("DemoWidget", "DemoGadget")],
        );
        assert_eq!(
            out,
            "DemoGadget b = new DemoGadget(); DemoWidgetEntity e; MyDemoWidget m;"
        );
    }

    #[test]
    fn test_skips_strings_comments_and_text_blocks() {
        let src = "// DemoWidget here\n/* DemoWidget */ String s = \"DemoWidget\"; char c = 'D';\n\
                   String t = \"\"\"\n  DemoWidget\n\"\"\"; DemoWidget x;";
        let out = rewrite(src, &[rule("DemoWidget", "DemoGadget")]);
        assert!(out.contains("// DemoWidget here"));
        assert!(out.contains("/* DemoWidget */"));
        assert!(out.contains("\"DemoWidget\""));
        assert!(out.contains("\n  DemoWidget\n"));
        assert!(out.ends_with("DemoGadget x;"));
    }

    #[test]
    fn test_escaped_quote_does_not_end_string() {
        let out = rewrite(
            r#"s = "a\"DemoWidget"; DemoWidget y;"#,
            &[rule("DemoWidget", "DemoGadget")],
        );
        assert_eq!(out, r#"s = "a\"DemoWidget"; DemoGadget y;"#);
    }

    /// ADR 0008: after a dot, a simple name is renamed only when everything
    /// before it in the dotted path is lowercase (a package path).
    #[test]
    fn test_after_dot_renamed_only_after_package_path() {
        let src = "import com.example.DemoWidget;\n\
                   com.example.DemoWidget.create(); Outer.DemoWidget o; foo().DemoWidget = 1; DemoWidget.getId();";
        let out = rewrite(src, &[rule("DemoWidget", "DemoGadget")]);
        assert_eq!(
            out,
            "import com.example.DemoGadget;\n\
             com.example.DemoGadget.create(); Outer.DemoWidget o; foo().DemoWidget = 1; DemoGadget.getId();"
        );
    }

    #[test]
    fn test_qualified_rule_wins_over_simple() {
        let src = "import com.example.old.DemoWidget;\nDemoWidget b;";
        let rules = [
            rule("DemoWidget", "DemoGadget"),
            rule("com.example.old.DemoWidget", "com.example.demo.DemoWidget"),
        ];
        let (out, reps) = TokenRenameTransformer.transform(src, &rules);
        assert_eq!(out, "import com.example.demo.DemoWidget;\nDemoGadget b;");
        assert_eq!(reps.len(), 2);
        assert_eq!(reps[0].line, 1);
        assert_eq!(reps[1].line, 2);
    }

    #[test]
    fn test_numeric_literals_untouched() {
        let out = rewrite("int x = 0xBlock; float f = 1.5f;", &[rule("xBlock", "Y")]);
        assert_eq!(out, "int x = 0xBlock; float f = 1.5f;");
    }

    #[test]
    fn test_applies_to_jvm_sources_only() {
        let t = TokenRenameTransformer;
        assert!(t.applies_to(Path::new("src/A.java")));
        assert!(t.applies_to(Path::new("src/A.kt")));
        assert!(t.applies_to(Path::new("src/A.groovy")));
        assert!(t.applies_to(Path::new("src/A.scala")));
        assert!(!t.applies_to(Path::new("build.gradle.kts")));
        assert!(!t.applies_to(Path::new("build.gradle")));
        assert!(!t.applies_to(Path::new("assets/lang/en_us.json")));
    }

    #[test]
    fn groovy_and_scala_renamed_not_in_triple_strings() {
        let rules = [
            rule("com.example.old.DemoWidget", "com.example.next.DemoGadget"),
            rule("DemoWidget", "DemoGadget"),
        ];
        let groovy = "import com.example.old.DemoWidget\n\
                      def w = new DemoWidget()\n\
                      def doc = '''\nDemoWidget stays here\n'''\n";
        let out = rewrite(groovy, &rules);
        assert!(out.contains("import com.example.next.DemoGadget"), "{out}");
        assert!(out.contains("new DemoGadget()"), "{out}");
        assert!(out.contains("DemoWidget stays here"), "{out}");
        let scala = "import com.example.old.DemoWidget\n\
                     val doc = \"\"\"\nDemoWidget stays\n\"\"\"\n\
                     val w: DemoWidget = new DemoWidget\n";
        let out = rewrite(scala, &rules);
        assert!(out.contains("val w: DemoGadget = new DemoGadget"), "{out}");
        assert!(out.contains("DemoWidget stays"), "{out}");
    }
}
