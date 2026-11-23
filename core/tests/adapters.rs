//! Phase 9 adapters: rule-based technology detection and file create/delete in edit sets.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ummp_core::adapter::{AdapterRegistry, Handling};
use ummp_core::checkpoint::CheckpointStore;
use ummp_core::migration::{EditSet, FileEdit, MigrationEngine, RunOptions};
use ummp_core::project::{DefaultProjectManager, ProjectManager};
use uuid::Uuid;

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn read_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    ummp_core::checkpoint::collect_files(root)
        .unwrap()
        .into_iter()
        .map(|r| (r.clone(), std::fs::read(root.join(&r)).unwrap()))
        .collect()
}

#[test]
fn rules_detect_each_technology() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let j = "src/main/java/com/example/demo";
    write(r, "src/main/kotlin/Demo.kt", "class Demo\n");
    write(r, "src/main/scala/Demo.scala", "object Demo\n");
    write(r, "build-logic/Demo.groovy", "class Demo {}\n");
    write(r, "native/CMakeLists.txt", "project(demo)\n");
    write(r, "native-rs/Cargo.toml", "[package]\nname = \"demo\"\n");
    write(
        r,
        &format!("{j}/DemoNative.java"),
        "class DemoNative {\n  // x\n  private native int size();\n}\n",
    );
    write(
        r,
        "src/main/resources/assets/demo/shaders/core/demo.fsh",
        "#version 150\n",
    );
    write(
        r,
        &format!("{j}/DemoRender.java"),
        "import com.mojang.blaze3d.vertex.PoseStack;\nclass DemoRender {}\n",
    );
    write(
        r,
        &format!("{j}/DemoPacket.java"),
        "class DemoPacket implements CustomPacketPayload {}\n",
    );
    write(
        r,
        &format!("{j}/DemoLoader.java"),
        "class DemoLoader extends ClassLoader {}\n",
    );
    write(
        r,
        &format!("{j}/mixin/DemoMixin.java"),
        "class DemoMixin {\n  @Redirect(method = \"x\")\n  void y() {}\n}\n",
    );
    write(
        r,
        "src/main/resources/demo.accesswidener",
        "accessWidener v2 named\n",
    );
    write(
        r,
        "src/main/resources/META-INF/accesstransformer.cfg",
        "public com.example.demo.DemoWidget\n",
    );
    write(r, "src/main/resources/META-INF/coremods.json", "{}\n");
    write(
        r,
        "common/build.gradle",
        "dependencies { modImplementation \"dev.architectury:architectury:1\" }\n",
    );
    write(r, &format!("{j}/Plain.java"), "class Plain {}\n");

    let found = AdapterRegistry::default().scan_dir(r);
    let by_tech: BTreeMap<&str, _> = found.iter().map(|f| (f.tech.as_str(), f)).collect();
    for tech in [
        "kotlin",
        "scala",
        "groovy",
        "c-cpp",
        "rust-native",
        "jni-jna",
        "shader",
        "rendering",
        "networking",
        "classloader",
        "mixin-advanced",
        "access-widener",
        "access-transformer",
        "coremod",
        "multi-loader",
    ] {
        assert!(by_tech.contains_key(tech), "missing {tech}: {found:#?}");
    }
    assert_eq!(found.iter().filter(|f| f.tech == "jni-jna").count(), 1);
    let jni = by_tech["jni-jna"];
    assert_eq!(jni.file, format!("{j}/DemoNative.java"));
    assert_eq!(jni.line, Some(3));
    assert_eq!(jni.handling, Handling::Manual);
    assert_eq!(by_tech["mixin-advanced"].line, Some(2));
    assert_eq!(by_tech["kotlin"].handling, Handling::Auto);
}

#[test]
fn plain_java_project_has_no_findings() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "src/main/java/com/example/demo/DemoWidget.java",
        "class DemoWidget {}\n",
    );
    assert!(AdapterRegistry::default().scan_dir(d.path()).is_empty());
}

#[test]
fn delete_and_create_apply_and_rollback() {
    let d = tempfile::tempdir().unwrap();
    let snaps = tempfile::tempdir().unwrap();
    write(d.path(), "fabric.mod.json", "{\"id\":\"demowidget\"}\n");
    write(d.path(), "src/Keep.java", "class Keep {}\n");
    let before = read_tree(d.path());
    let project = DefaultProjectManager.open(d.path()).unwrap();
    let set = EditSet {
        edits: vec![FileEdit {
            path: PathBuf::from("META-INF/neoforge.mods.toml"),
            original: String::new(),
            updated: "modLoader = \"javafml\"\n".into(),
            replacements: vec![],
        }],
        deletes: vec![PathBuf::from("fabric.mod.json")],
        ..EditSet::default()
    };
    let mut store = CheckpointStore::new();
    let id = MigrationEngine::default()
        .apply_edit_set(
            &set,
            &project,
            &mut store,
            &RunOptions {
                dry_run: false,
                snapshot_root: snaps.path(),
                session_id: Uuid::new_v4(),
            },
        )
        .unwrap()
        .expect("checkpoint");
    assert!(d.path().join("META-INF/neoforge.mods.toml").exists());
    assert!(!d.path().join("fabric.mod.json").exists());
    store.restore(id, d.path()).unwrap();
    assert_eq!(read_tree(d.path()), before);
}

#[test]
fn native_lists_missing_platforms() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "src/main/resources/natives/windows-x86_64/demo.dll",
        "MZ",
    );
    write(
        d.path(),
        "src/main/resources/natives/linux-x86_64/libdemo.so",
        "\x7fELF",
    );
    let found = AdapterRegistry::default().scan_dir(d.path());
    let f = found
        .iter()
        .find(|f| f.tech == "native-libs")
        .expect("native finding");
    assert_eq!(f.handling, Handling::Manual);
    assert!(f.message.contains("macos"), "{}", f.message);
    assert!(!f.message.contains("missing: windows"), "{}", f.message);
}

#[test]
fn native_methods_and_load_library_reported() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "src/main/java/com/example/demo/DemoNative.java",
        "class DemoNative {\n  static { System.loadLibrary(\"demo\"); }\n  static native int size();\n}\n",
    );
    write(d.path(), "src/main/resources/natives/demo.dll", "MZ");
    let found = AdapterRegistry::default().scan_dir(d.path());
    let f = found
        .iter()
        .find(|f| f.tech == "native-libs" && f.message.contains("'demo'"))
        .expect("loadLibrary finding");
    assert_eq!(f.line, Some(2));
    assert!(
        f.message.contains("linux") && f.message.contains("macos"),
        "{}",
        f.message
    );
}

// ---- Mixin member renames (Task 3) ----

const MIXIN_STUBS: &[(&str, &str)] = &[
    ("org/spongepowered/asm/mixin/Mixin.java", "package org.spongepowered.asm.mixin;\npublic @interface Mixin { Class<?>[] value() default {}; String[] targets() default {}; }\n"),
    ("org/spongepowered/asm/mixin/gen/Accessor.java", "package org.spongepowered.asm.mixin.gen;\npublic @interface Accessor { String value() default \"\"; }\n"),
    ("org/spongepowered/asm/mixin/gen/Invoker.java", "package org.spongepowered.asm.mixin.gen;\npublic @interface Invoker { String value() default \"\"; }\n"),
    ("org/spongepowered/asm/mixin/injection/At.java", "package org.spongepowered.asm.mixin.injection;\npublic @interface At { String value(); String target() default \"\"; }\n"),
    ("org/spongepowered/asm/mixin/injection/Inject.java", "package org.spongepowered.asm.mixin.injection;\npublic @interface Inject { String[] method(); At[] at(); }\n"),
    ("com/example/demo/DemoWidget.java", "package com.example.demo;\npublic class DemoWidget { int demoSize; void tickDemo() {} void runDemo() {} }\n"),
    ("com/example/demo/OtherWidget.java", "package com.example.demo;\npublic class OtherWidget { void tickDemo() {} }\n"),
];

const DEMO_MIXIN: &str = "package com.example.demo.mixin;\n\
import com.example.demo.DemoWidget;\n\
import org.spongepowered.asm.mixin.Mixin;\n\
import org.spongepowered.asm.mixin.gen.Accessor;\n\
import org.spongepowered.asm.mixin.gen.Invoker;\n\
import org.spongepowered.asm.mixin.injection.At;\n\
import org.spongepowered.asm.mixin.injection.Inject;\n\
@Mixin(DemoWidget.class)\n\
public interface DemoWidgetMixin {\n\
    @Accessor(\"demoSize\") int ummpSize();\n\
    @Invoker(\"runDemo\") void ummpRun();\n\
    @Inject(method = \"tickDemo\", at = @At(value = \"INVOKE\", target = \"Lcom/example/demo/DemoWidget;runDemo()V\"))\n\
    default void ummpTick() {}\n\
}\n";

fn member_rules() -> Vec<ummp_core::migration::RenameRule> {
    [
        (
            "com.example.demo.DemoWidget#tickDemo",
            "com.example.demo.DemoWidget#tickWidget",
        ),
        (
            "com.example.demo.DemoWidget#runDemo",
            "com.example.demo.DemoWidget#runWidget",
        ),
        (
            "com.example.demo.DemoWidget#demoSize",
            "com.example.demo.DemoWidget#widgetSize",
        ),
    ]
    .iter()
    .map(|(f, t)| ummp_core::migration::RenameRule {
        from: (*f).into(),
        to: (*t).into(),
        kind: "method".into(),
        confidence: 1.0,
        version: "1.21.1".into(),
    })
    .collect()
}

fn javac_ok(extra: &[(&str, &str)]) -> bool {
    let d = tempfile::tempdir().unwrap();
    let mut files = Vec::new();
    for (rel, body) in MIXIN_STUBS.iter().chain(extra) {
        write(d.path(), &format!("src/{rel}"), body);
        files.push(d.path().join("src").join(rel));
    }
    std::process::Command::new("javac")
        .arg("-d")
        .arg(d.path().join("out"))
        .args(&files)
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn member_rename_inject_accessor_invoker_at() {
    let r = ummp_core::mixin::migrate_mixin_members(DEMO_MIXIN, &member_rules());
    let out = &r.updated;
    assert!(out.contains("@Accessor(\"widgetSize\")"), "{out}");
    assert!(out.contains("@Invoker(\"runWidget\")"), "{out}");
    assert!(out.contains("method = \"tickWidget\""), "{out}");
    assert!(
        out.contains("target = \"Lcom/example/demo/DemoWidget;runWidget()V\""),
        "{out}"
    );
    assert_eq!(r.replacements.len(), 4);
    if std::process::Command::new("javac")
        .arg("-version")
        .output()
        .is_ok()
    {
        let rel = "com/example/demo/mixin/DemoWidgetMixin.java";
        assert!(
            javac_ok(&[(rel, DEMO_MIXIN)]),
            "original fixture must compile"
        );
        assert!(javac_ok(&[(rel, out)]), "renamed source must compile");
    }
}

#[test]
fn member_rename_owner_guard() {
    let other = DEMO_MIXIN
        .replace(
            "import com.example.demo.DemoWidget;",
            "import com.example.demo.OtherWidget;",
        )
        .replace("@Mixin(DemoWidget.class)", "@Mixin(OtherWidget.class)")
        .replace(
            "Lcom/example/demo/DemoWidget;",
            "Lcom/example/demo/OtherWidget;",
        );
    let r = ummp_core::mixin::migrate_mixin_members(&other, &member_rules());
    assert_eq!(r.updated, other);
    assert!(r.replacements.is_empty());
}
