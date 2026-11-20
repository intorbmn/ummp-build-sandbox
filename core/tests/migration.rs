//! Phase 6 regression tests: plan → transform → diff → apply → rollback on
//! the `tests/fixtures/forge-src-1.16` corpus project.
//!
//! The mappings below are test data describing the Forge 1.16.5 (MCP) →
//! 1.17 (official names) package moves; they are not knowledge-base seed data.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ummp_core::checkpoint::{collect_files, CheckpointStore};
use ummp_core::filesystem::{FileSystem, FsError, LocalFileSystem};
use ummp_core::knowledge::{BreakingChange, Compatibility, KnowledgeBase, Mapping, Recipe};
use ummp_core::migration::{
    MigrationEngine, MigrationError, MigrationPlanner, MigrationRequest, RunOptions,
};
use ummp_core::project::{DefaultProjectManager, Project, ProjectManager};
use uuid::Uuid;

const FIXTURE: &str = "../tests/fixtures/forge-src-1.16";
const GOLDEN: &str = "../tests/fixtures/golden/ExampleMod-1.17.java.golden";
const MOD_JAVA: &str = "src/main/java/com/example/examplemod/ExampleMod.java";
const MODS_TOML: &str = "src/main/resources/META-INF/mods.toml";
const GOLDEN_TOML: &str = "../tests/fixtures/golden/mods-1.17.toml.golden";

fn copy_tree(from: &Path, to: &Path) {
    for rel in collect_files(from).unwrap() {
        let dest = to.join(&rel);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(from.join(&rel), dest).unwrap();
    }
}

fn read_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    collect_files(root)
        .unwrap()
        .into_iter()
        .map(|rel| {
            let bytes = fs::read(root.join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect()
}

fn mapping(version: &str, from: &str, to: &str, confidence: f64) -> Mapping {
    Mapping {
        version: version.into(),
        loader: "forge".into(),
        from_api: from.into(),
        to_api: to.into(),
        kind: "move".into(),
        source: "test".into(),
        confidence,
    }
}

fn forge_117_kb(dir: &Path) -> KnowledgeBase {
    let kb = KnowledgeBase::open(&dir.join("kb.db")).unwrap();
    for m in [
        mapping(
            "1.17",
            "net.minecraft.block.Block",
            "net.minecraft.world.level.block.Block",
            0.95,
        ),
        mapping(
            "1.17",
            "net.minecraft.block.AbstractBlock",
            "net.minecraft.world.level.block.state.BlockBehaviour",
            0.95,
        ),
        mapping("1.17", "AbstractBlock", "BlockBehaviour", 0.9),
        mapping(
            "1.17",
            "net.minecraft.block.material.Material",
            "net.minecraft.world.level.material.Material",
            0.95,
        ),
        mapping(
            "1.17",
            "net.minecraft.item.Item",
            "net.minecraft.world.item.Item",
            0.95,
        ),
        mapping(
            "1.17",
            "net.minecraft.item.BlockItem",
            "net.minecraft.world.item.BlockItem",
            0.95,
        ),
        // Below the threshold: must be reported, never applied.
        mapping("1.17", "EXAMPLE_BLOCK", "SHOULD_NOT_APPEAR", 0.4),
        // Outside the (1.16.5, 1.17.1] range: ignored entirely.
        mapping("1.20", "Item", "SHOULD_NOT_APPEAR_EITHER", 0.99),
    ] {
        kb.insert_mapping(&m).unwrap();
    }
    kb.insert_breaking_change(&BreakingChange {
        version: "1.17".into(),
        change: "Java 16 required".into(),
        severity: "high".into(),
        workaround: Some("Update the Gradle toolchain to Java 16+".into()),
        source: "test".into(),
    })
    .unwrap();
    // Test data: Forge 1.17.1 shipped as 37.x.
    kb.insert_compatibility(&Compatibility {
        minecraft_version: "1.17.1".into(),
        loader: "forge".into(),
        loader_version: "37.1.1".into(),
        status: "stable".into(),
        notes: None,
    })
    .unwrap();
    // Test data: 1.17.x resource and data pack format 7.
    kb.insert_pack_format("1.17.1", "resource", 7).unwrap();
    kb.insert_pack_format("1.17.1", "data", 7).unwrap();
    kb
}

fn read(root: &Path, rel: &str) -> String {
    fs::read_to_string(root.join(rel)).unwrap()
}

fn request() -> MigrationRequest {
    MigrationRequest {
        source_minecraft: "1.16.5".into(),
        target_minecraft: "1.17.1".into(),
        target_loader: "forge".into(),
        min_confidence: 0.8,
        source_loader: None,
    }
}

/// Phase 6 acceptance: a corpus mod migrates through the transform step and
/// produces a valid diff; rollback restores the original exactly.
#[test]
fn test_forge_116_to_117_dry_run_apply_rollback() {
    let work = tempfile::tempdir().unwrap();
    let snaps = tempfile::tempdir().unwrap();
    let kb_dir = tempfile::tempdir().unwrap();
    copy_tree(Path::new(FIXTURE), work.path());
    let original = read_tree(work.path());

    let kb = forge_117_kb(kb_dir.path());
    let plan = MigrationPlanner::plan(&kb, &request()).unwrap();
    assert_eq!(plan.stages.len(), 1);
    assert_eq!(plan.rule_count(), 6);
    assert_eq!(plan.skipped.len(), 1);
    assert_eq!(plan.breaking_changes.len(), 1);

    let project = DefaultProjectManager.open(work.path()).unwrap();
    let engine = MigrationEngine::default();
    let mut store = CheckpointStore::new();

    // Dry run: report + diff, nothing written.
    let dry = engine
        .run(
            &plan,
            &project,
            &mut store,
            &RunOptions {
                dry_run: true,
                snapshot_root: snaps.path(),
                session_id: Uuid::new_v4(),
            },
        )
        .unwrap();
    assert_eq!(read_tree(work.path()), original);
    assert!(dry.checkpoint_id.is_none());
    // ExampleMod.java (6) + BlockMixin.java (2) + mods.toml (3) + gradle.properties (5)
    // + build.gradle (1) + examplemod.mixins.json (1) + pack.mcmeta (1)
    assert_eq!(dry.changes.len(), 7);
    assert_eq!(dry.total_replacements(), 19);
    assert_eq!(dry.warnings.len(), 2, "{:?}", dry.warnings);
    assert!(dry.warnings.iter().any(|w| w.contains("mapping_version")));
    assert!(dry
        .warnings
        .iter()
        .any(|w| w.contains("injector method target")));
    let java = dry
        .changes
        .iter()
        .find(|c| c.path == Path::new(MOD_JAVA))
        .unwrap();
    let diff = &java.diff;
    assert!(diff.starts_with(&format!("--- a/{MOD_JAVA}\n+++ b/{MOD_JAVA}\n")));
    assert!(diff.contains("-import net.minecraft.block.Block;\n"));
    assert!(diff.contains("+import net.minecraft.world.level.block.Block;\n"));
    let md = dry.to_markdown();
    assert!(md.contains("Java 16 required"));
    assert!(md.contains("EXAMPLE_BLOCK"));
    assert!(dry.to_json().unwrap().contains("\"dry_run\": true"));

    // Apply: matches the golden file; resources untouched.
    let applied = engine
        .run(
            &plan,
            &project,
            &mut store,
            &RunOptions {
                dry_run: false,
                snapshot_root: snaps.path(),
                session_id: Uuid::new_v4(),
            },
        )
        .unwrap();
    let cp_id = applied.checkpoint_id.expect("apply must checkpoint first");
    let migrated = fs::read_to_string(work.path().join(MOD_JAVA)).unwrap();
    let golden = fs::read_to_string(GOLDEN).unwrap();
    assert_eq!(migrated.replace("\r\n", "\n"), golden.replace("\r\n", "\n"));
    assert!(!migrated.contains("SHOULD_NOT_APPEAR"));
    let toml = fs::read_to_string(work.path().join(MODS_TOML)).unwrap();
    let golden_toml = fs::read_to_string(GOLDEN_TOML).unwrap();
    assert_eq!(
        toml.replace("\r\n", "\n"),
        golden_toml.replace("\r\n", "\n")
    );
    let w = work.path();
    let props = read(w, "gradle.properties");
    assert!(props.contains("minecraft_version=1.17.1\nminecraft_version_range=[1.17.1,1.18)\n"));
    assert!(props
        .contains("forge_version=37.1.1\nforge_version_range=[37,)\nloader_version_range=[37,)\n"));
    assert!(props.contains("mapping_version=20210309-1.16.5")); // reported, not guessed
    assert!(read(w, "build.gradle").contains("JavaLanguageVersion.of(16)"));
    assert!(read(w, "src/main/resources/examplemod.mixins.json").contains("\"JAVA_16\""));
    assert!(read(w, "src/main/resources/pack.mcmeta").contains("\"pack_format\": 7"));
    let mixin = read(
        w,
        "src/main/java/com/example/examplemod/mixin/BlockMixin.java",
    );
    assert!(mixin.contains("import net.minecraft.world.level.block.Block;"));
    assert!(mixin.contains("onPlace(Lnet/minecraft/world/level/block/Block;)V"));
    let lang = "src/main/resources/assets/examplemod/lang/en_us.json";
    assert_eq!(
        fs::read(work.path().join(lang)).unwrap(),
        original[Path::new(lang)]
    );

    // Rollback: byte-identical to the original tree.
    store.restore(cp_id, &project.root).unwrap();
    assert_eq!(read_tree(work.path()), original);
}

#[test]
fn test_stages_run_in_version_order() {
    let kb_dir = tempfile::tempdir().unwrap();
    let kb = KnowledgeBase::open(&kb_dir.path().join("kb.db")).unwrap();
    // Inserted out of order on purpose.
    kb.insert_mapping(&mapping("1.18", "Mid", "Final", 1.0))
        .unwrap();
    kb.insert_mapping(&mapping("1.17", "Old", "Mid", 1.0))
        .unwrap();

    let work = tempfile::tempdir().unwrap();
    fs::write(work.path().join("A.java"), "class A { Old o; }").unwrap();
    let project = DefaultProjectManager.open(work.path()).unwrap();

    let mut req = request();
    req.target_minecraft = "1.18.2".into();
    let plan = MigrationPlanner::plan(&kb, &req).unwrap();
    let versions: Vec<&str> = plan.stages.iter().map(|s| s.version.as_str()).collect();
    assert_eq!(versions, ["1.17", "1.18"]);

    let set = MigrationEngine::default()
        .compute_edits(&plan, &project)
        .unwrap();
    assert_eq!(set.edits[0].updated, "class A { Final o; }");
}

#[test]
fn test_conflicting_mappings_are_not_applied() {
    let kb_dir = tempfile::tempdir().unwrap();
    let kb = KnowledgeBase::open(&kb_dir.path().join("kb.db")).unwrap();
    kb.insert_mapping(&mapping("1.17", "Thing", "ThingA", 1.0))
        .unwrap();
    kb.insert_mapping(&mapping("1.17", "Thing", "ThingB", 1.0))
        .unwrap();
    let plan = MigrationPlanner::plan(&kb, &request()).unwrap();
    assert_eq!(plan.rule_count(), 0);
    assert_eq!(plan.skipped.len(), 2);
}

#[test]
fn test_downgrade_and_non_numeric_versions_rejected() {
    let kb_dir = tempfile::tempdir().unwrap();
    let kb = KnowledgeBase::open(&kb_dir.path().join("kb.db")).unwrap();
    let mut req = request();
    req.target_minecraft = "1.12.2".into();
    assert!(matches!(
        MigrationPlanner::plan(&kb, &req),
        Err(MigrationError::InvalidRequest(_))
    ));
    req.target_minecraft = "latest".into();
    assert!(matches!(
        MigrationPlanner::plan(&kb, &req),
        Err(MigrationError::InvalidRequest(_))
    ));
}

/// Succeeds for the first `ok_writes` writes, then fails.
struct FailingFs {
    ok_writes: usize,
    writes: AtomicUsize,
}

impl FileSystem for FailingFs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, FsError> {
        LocalFileSystem.read(path)
    }
    fn write(&self, path: &Path, data: &[u8]) -> Result<(), FsError> {
        if self.writes.fetch_add(1, Ordering::SeqCst) >= self.ok_writes {
            return Err(FsError::LimitExceeded("simulated disk full".into()));
        }
        LocalFileSystem.write(path, data)
    }
    fn remove(&self, path: &Path) -> Result<(), FsError> {
        LocalFileSystem.remove(path)
    }
    fn list(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        LocalFileSystem.list(path)
    }
    fn exists(&self, path: &Path) -> bool {
        LocalFileSystem.exists(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        LocalFileSystem.is_dir(path)
    }
}

#[test]
fn test_failed_apply_rolls_back_partial_writes() {
    let work = tempfile::tempdir().unwrap();
    let snaps = tempfile::tempdir().unwrap();
    let kb_dir = tempfile::tempdir().unwrap();
    fs::write(work.path().join("A.java"), "class A { Old o; }").unwrap();
    fs::write(work.path().join("B.java"), "class B { Old o; }").unwrap();
    let original = read_tree(work.path());

    let kb = KnowledgeBase::open(&kb_dir.path().join("kb.db")).unwrap();
    kb.insert_mapping(&mapping("1.17", "Old", "New", 1.0))
        .unwrap();
    let plan = MigrationPlanner::plan(&kb, &request()).unwrap();

    let project = Project {
        root: work.path().canonicalize().unwrap(),
        fs: Arc::new(FailingFs {
            ok_writes: 1,
            writes: AtomicUsize::new(0),
        }),
    };
    let err = MigrationEngine::default()
        .run(
            &plan,
            &project,
            &mut CheckpointStore::new(),
            &RunOptions {
                dry_run: false,
                snapshot_root: snaps.path(),
                session_id: Uuid::new_v4(),
            },
        )
        .unwrap_err();

    assert!(matches!(
        err,
        MigrationError::ApplyFailed {
            rolled_back: true,
            ..
        }
    ));
    assert_eq!(read_tree(work.path()), original);
}

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

/// Phase 6.D: 1.21 data pack folder renames are applied as moves, the pack
/// format comes from the KB, and rollback restores the old layout exactly.
#[test]
fn test_datapack_moves_pack_format_and_rollback() {
    let work = tempfile::tempdir().unwrap();
    let snaps = tempfile::tempdir().unwrap();
    let kb_dir = tempfile::tempdir().unwrap();
    let w = work.path();
    let res = "src/main/resources";
    write(
        w,
        &format!("{res}/pack.mcmeta"),
        "{\"pack\":{\"pack_format\":26,\"description\":\"demo\"}}\n",
    );
    write(
        w,
        &format!("{res}/data/demo/recipes/widget.json"),
        "{\"type\":\"demo\"}\n",
    );
    write(
        w,
        &format!("{res}/data/demo/tags/items/widgets.json"),
        "{\"values\":[]}\n",
    );
    write(w, &format!("{res}/data/demo/worldgen/biome/x.json"), "{}\n");
    let original = read_tree(w);

    let kb = KnowledgeBase::open(&kb_dir.path().join("kb.db")).unwrap();
    kb.insert_pack_format("1.21", "data", 48).unwrap(); // test data
    let req = MigrationRequest {
        source_minecraft: "1.20.4".into(),
        target_minecraft: "1.21".into(),
        target_loader: "fabric".into(),
        min_confidence: 0.8,
        source_loader: None,
    };
    let plan = MigrationPlanner::plan(&kb, &req).unwrap();
    assert!(plan.datapack_renames);
    assert_eq!(plan.java, Some(21));

    let project = DefaultProjectManager.open(w).unwrap();
    let mut store = CheckpointStore::new();
    let report = MigrationEngine::default()
        .run(
            &plan,
            &project,
            &mut store,
            &RunOptions {
                dry_run: false,
                snapshot_root: snaps.path(),
                session_id: Uuid::new_v4(),
            },
        )
        .unwrap();
    assert_eq!(report.moves.len(), 2);
    assert!(w
        .join(format!("{res}/data/demo/recipe/widget.json"))
        .exists());
    assert!(w
        .join(format!("{res}/data/demo/tags/item/widgets.json"))
        .exists());
    assert!(!w.join(format!("{res}/data/demo/recipes")).exists());
    assert!(!w.join(format!("{res}/data/demo/tags/items")).exists());
    assert!(w
        .join(format!("{res}/data/demo/worldgen/biome/x.json"))
        .exists());
    // data-only pack (no assets/): pack_format is the data pack format
    assert!(read(w, &format!("{res}/pack.mcmeta")).contains("\"pack_format\":48"));

    store
        .restore(report.checkpoint_id.unwrap(), &project.root)
        .unwrap();
    assert_eq!(read_tree(w), original);
}

/// Phase 6.G: tested recipes run (rename + warn); others are reported.
#[test]
fn test_recipes_executed_only_when_tested() {
    let kb_dir = tempfile::tempdir().unwrap();
    let kb = KnowledgeBase::open(&kb_dir.path().join("kb.db")).unwrap();
    let recipe = |id: &str, tested: bool, steps: Vec<serde_json::Value>| Recipe {
        recipe_id: id.into(),
        version: "1.17".into(),
        loader: Some("forge".into()),
        description: format!("{id} description"),
        steps,
        source: "test".into(),
        tested,
        recipe_version: 1,
        parent_recipe_id: None,
    };
    kb.insert_recipe(&recipe(
        "demo-tested",
        true,
        vec![
            serde_json::json!({"op": "rename", "from": "DemoWidget", "to": "DemoGadget"}),
            serde_json::json!({"op": "warn", "message": "check DemoGadget registration"}),
        ],
    ))
    .unwrap();
    kb.insert_recipe(&recipe(
        "demo-untested",
        false,
        vec![serde_json::json!({"op": "rename", "from": "DemoOther", "to": "DemoThing"})],
    ))
    .unwrap();
    kb.insert_recipe(&recipe(
        "demo-legacy",
        true,
        vec![serde_json::json!("replace: x")],
    ))
    .unwrap();

    let plan = MigrationPlanner::plan(&kb, &request()).unwrap();
    let outcome = |id: &str| plan.recipes.iter().find(|o| o.recipe_id == id).unwrap();
    assert!(outcome("demo-tested").executed);
    assert_eq!(outcome("demo-tested").note, "check DemoGadget registration");
    assert!(!outcome("demo-untested").executed);
    assert!(!outcome("demo-legacy").executed);

    let work = tempfile::tempdir().unwrap();
    fs::write(
        work.path().join("A.java"),
        "class A { DemoWidget w; DemoOther o; }",
    )
    .unwrap();
    let project = DefaultProjectManager.open(work.path()).unwrap();
    let set = MigrationEngine::default()
        .compute_edits(&plan, &project)
        .unwrap();
    assert_eq!(
        set.edits[0].updated,
        "class A { DemoGadget w; DemoOther o; }"
    );
}

/// A move that fails mid-apply restores the checkpoint.
#[test]
fn test_failed_move_rolls_back() {
    let work = tempfile::tempdir().unwrap();
    let snaps = tempfile::tempdir().unwrap();
    let kb_dir = tempfile::tempdir().unwrap();
    write(work.path(), "data/demo/recipes/a.json", "{}");
    write(work.path(), "data/demo/loot_tables/b.json", "{}");
    let original = read_tree(work.path());
    let kb = KnowledgeBase::open(&kb_dir.path().join("kb.db")).unwrap();
    let req = MigrationRequest {
        source_minecraft: "1.20.4".into(),
        target_minecraft: "1.21".into(),
        target_loader: "fabric".into(),
        min_confidence: 0.8,
        source_loader: None,
    };
    let plan = MigrationPlanner::plan(&kb, &req).unwrap();
    let project = Project {
        root: work.path().canonicalize().unwrap(),
        fs: Arc::new(FailingFs {
            ok_writes: 1,
            writes: AtomicUsize::new(0),
        }),
    };
    let err = MigrationEngine::default()
        .run(
            &plan,
            &project,
            &mut CheckpointStore::new(),
            &RunOptions {
                dry_run: false,
                snapshot_root: snaps.path(),
                session_id: Uuid::new_v4(),
            },
        )
        .unwrap_err();
    assert!(matches!(
        err,
        MigrationError::ApplyFailed {
            rolled_back: true,
            ..
        }
    ));
    assert_eq!(read_tree(work.path()), original);
}
