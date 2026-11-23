//! Phase 9 loader conversion: Fabric ↔ NeoForge, Forge → NeoForge, through the real planner
//! and `MigrationEngine` (checkpointed apply, rollback).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ummp_core::adapter::Handling;
use ummp_core::checkpoint::CheckpointStore;
use ummp_core::knowledge::KnowledgeBase;
use ummp_core::migration::{
    MigrationEngine, MigrationPlanner, MigrationReport, MigrationRequest, RunOptions,
};
use ummp_core::project::{DefaultProjectManager, ProjectManager};
use uuid::Uuid;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures")
}

fn copy_tree(from: &Path, to: &Path) {
    for rel in ummp_core::checkpoint::collect_files(from).unwrap() {
        let dest = to.join(&rel);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::copy(from.join(&rel), dest).unwrap();
    }
}

fn read_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    ummp_core::checkpoint::collect_files(root)
        .unwrap()
        .into_iter()
        .map(|r| (r.clone(), std::fs::read(root.join(&r)).unwrap()))
        .collect()
}

fn golden(name: &str) -> String {
    std::fs::read_to_string(fixtures().join("golden/loader").join(name)).unwrap()
}

/// Copies fixture `src`, converts it to `to`, applies; returns (work dir, scratch, report, store).
type Converted = (
    tempfile::TempDir,
    tempfile::TempDir,
    MigrationReport,
    CheckpointStore,
);

fn convert(src: &str, from: &str, to: &str) -> Converted {
    convert_to(src, from, to, "1.21.1")
}

fn convert_to(src: &str, from: &str, to: &str, mc: &str) -> Converted {
    let work = tempfile::tempdir().unwrap();
    let snaps = tempfile::tempdir().unwrap();
    copy_tree(&fixtures().join("loader").join(src), work.path());
    let kb = KnowledgeBase::open(&snaps.path().join("kb.db")).unwrap();
    let plan = MigrationPlanner::plan(
        &kb,
        &MigrationRequest {
            source_minecraft: "1.21.1".into(),
            target_minecraft: mc.into(),
            target_loader: to.into(),
            min_confidence: 0.8,
            source_loader: Some(from.into()),
        },
    )
    .unwrap();
    let project = DefaultProjectManager.open(work.path()).unwrap();
    let mut store = CheckpointStore::new();
    let report = MigrationEngine::default()
        .run(
            &plan,
            &project,
            &mut store,
            &RunOptions {
                dry_run: false,
                snapshot_root: &snaps.path().join("cp"),
                session_id: Uuid::new_v4(),
            },
        )
        .unwrap();
    (work, snaps, report, store)
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

const RES: &str = "src/main/resources";
const JAVA: &str = "src/main/java/com/example/demo";

#[test]
fn fabric_to_neoforge_golden() {
    let (w, _s, report, _) = convert("fabric-src", "fabric", "neoforge");
    let w = w.path();
    assert!(!w.join(RES).join("fabric.mod.json").exists());
    assert_eq!(
        read(w, &format!("{RES}/META-INF/neoforge.mods.toml")),
        golden("fabric-to-neoforge.neoforge.mods.toml")
    );
    assert_eq!(
        read(w, &format!("{JAVA}/UmmpNeoForgeEntry.java")),
        golden("fabric-to-neoforge.UmmpNeoForgeEntry.java")
    );
    assert_eq!(
        read(w, &format!("{RES}/META-INF/accesstransformer.cfg")),
        golden("fabric-to-neoforge.accesstransformer.cfg")
    );
    assert!(!w.join(RES).join("demowidget.accesswidener").exists());
    let main = read(w, &format!("{JAVA}/DemoWidgetMod.java"));
    assert!(!main.contains("ModInitializer"), "{main}");
    assert!(!main.contains("@Override"), "{main}");
    assert!(
        main.contains("public class DemoWidgetMod implements DemoApi {"),
        "{main}"
    );
    // Mixin config is kept as is.
    assert!(w.join(RES).join("demowidget.mixins.json").exists());
    let t = &report.technologies;
    let leftover = t
        .iter()
        .find(|f| f.tech == "loader-conversion" && f.file.ends_with("DemoEvents.java"))
        .expect("leftover Fabric API reported");
    assert_eq!(leftover.handling, Handling::Assisted);
    assert_eq!(leftover.line, Some(3));
    assert!(t
        .iter()
        .any(|f| f.handling == Handling::Manual && f.message.contains("fabric-api")));
    assert!(t
        .iter()
        .any(|f| f.handling == Handling::Manual && f.message.contains("demolib")));
    assert!(!t
        .iter()
        .any(|f| f.tech == "loader-conversion" && f.file.ends_with("DemoWidgetMod.java")));
}

#[test]
fn strip_keeps_other_interfaces() {
    let (w, _s, _, _) = convert("fabric-src", "fabric", "neoforge");
    let main = read(w.path(), &format!("{JAVA}/DemoWidgetMod.java"));
    assert!(main.contains("implements DemoApi"), "{main}");
    assert!(main.contains("public void onInitialize()"), "{main}");
}

#[test]
fn neoforge_to_fabric_golden() {
    let (w, _s, _, _) = convert("neoforge-src", "neoforge", "fabric");
    let w = w.path();
    assert!(!w.join(RES).join("META-INF/neoforge.mods.toml").exists());
    assert_eq!(
        read(w, &format!("{RES}/fabric.mod.json")),
        golden("neoforge-to-fabric.fabric.mod.json")
    );
    assert_eq!(
        read(w, &format!("{JAVA}/UmmpFabricEntry.java")),
        golden("neoforge-to-fabric.UmmpFabricEntry.java")
    );
    assert_eq!(
        read(w, &format!("{RES}/demowidget.accesswidener")),
        golden("neoforge-to-fabric.demowidget.accesswidener")
    );
    assert!(!w.join(RES).join("META-INF/accesstransformer.cfg").exists());
    let main = read(w, &format!("{JAVA}/DemoWidgetMod.java"));
    assert!(
        !main.contains("@Mod") && !main.contains("net.neoforged"),
        "{main}"
    );
}

#[test]
fn forge_to_neoforge_golden() {
    let (w, _s, report, _) = convert("forge-src", "forge", "neoforge");
    let w = w.path();
    assert!(!w.join(RES).join("META-INF/mods.toml").exists());
    let toml = read(w, &format!("{RES}/META-INF/neoforge.mods.toml"));
    assert!(toml.contains("modId = \"neoforge\""), "{toml}");
    let main = read(w, &format!("{JAVA}/DemoWidgetMod.java"));
    assert!(
        main.contains("import net.neoforged.fml.common.Mod;"),
        "{main}"
    );
    assert!(
        main.contains("import net.neoforged.bus.api.IEventBus;"),
        "{main}"
    );
    assert!(
        main.contains("import net.neoforged.neoforge.common.NeoForge;"),
        "{main}"
    );
    assert!(
        main.contains("NeoForge.EVENT_BUS.register(this);"),
        "{main}"
    );
    assert!(
        !w.join(JAVA).join("UmmpNeoForgeEntry.java").exists(),
        "no bridge between (Neo)Forge"
    );
    let left = report
        .technologies
        .iter()
        .find(|f| f.tech == "loader-conversion" && f.message.contains("FMLJavaModLoadingContext"))
        .expect("FMLJavaModLoadingContext reported");
    assert_eq!(left.handling, Handling::Assisted);
}

#[test]
fn conversion_rollback_identical() {
    let original = read_tree(&fixtures().join("loader/fabric-src"));
    let (w, _s, report, store) = convert("fabric-src", "fabric", "neoforge");
    assert_ne!(read_tree(w.path()), original);
    store
        .restore(report.checkpoint_id.unwrap(), w.path())
        .unwrap();
    assert_eq!(read_tree(w.path()), original);
}

#[test]
fn template_applied_for_known_version() {
    let (w, _s, report, _) = convert("fabric-src", "fabric", "neoforge");
    let w = w.path();
    let build = read(w, "build.gradle");
    assert!(
        build.contains("id 'net.neoforged.moddev' version '2.0.148'"),
        "{build}"
    );
    assert!(!build.contains("fabric-loom"), "{build}");
    let props = read(w, "gradle.properties");
    for want in [
        "neo_version=21.1.267",
        "mod_id=demowidget",
        "mod_version=1.0.0",
        "mod_group_id=com.example.demo",
    ] {
        assert!(props.contains(want), "missing {want}: {props}");
    }
    assert!(read(w, "settings.gradle").contains("rootProject.name = 'demowidget'"));
    assert!(!report
        .technologies
        .iter()
        .any(|f| f.message.contains("no build template")));
    // No wrapper in the fixture: the user is told which Gradle to add.
    assert!(report
        .technologies
        .iter()
        .any(|f| f.handling == Handling::Manual && f.message.contains("Gradle 9.2.1")));

    let (w2, _s2, _, _) = convert("neoforge-src", "neoforge", "fabric");
    let build = read(w2.path(), "build.gradle");
    assert!(build.contains("id 'fabric-loom' version"), "{build}");
    assert!(
        build.contains("accessWidenerPath = file(\"src/main/resources/demowidget.accesswidener\")"),
        "{build}"
    );
}

#[test]
fn no_template_is_manual() {
    let (w, _s, report, _) = convert_to("fabric-src", "fabric", "neoforge", "1.21.4");
    assert!(read(w.path(), "build.gradle").contains("fabric-loom"));
    assert!(w
        .path()
        .join("src/main/resources/META-INF/neoforge.mods.toml")
        .exists());
    assert!(report
        .technologies
        .iter()
        .any(|f| f.handling == Handling::Manual
            && f.message.contains("no build template for NeoForge 1.21.4")));
}
