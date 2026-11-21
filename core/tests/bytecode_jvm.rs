//! Phase 6.F end-to-end check against a real JVM: compile classes with
//! `javac`, remap the JAR, then compile and run code against the remapped
//! names. Skipped (with a message) when no JDK is installed.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use ummp_core::bytecode::{remap_jar, ClassMap};
use ummp_core::migration::RenameRule;

fn have(tool: &str) -> bool {
    Command::new(tool).arg("-version").output().is_ok()
}

fn javac(args: &[&str]) {
    let out = Command::new("javac").args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

#[test]
fn test_remapped_jar_links_and_runs_on_jvm() {
    if !have("javac") || !have("java") {
        eprintln!("skipped: no JDK on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    write(d, "src/com/example/old/DemoWidget.java",
        "package com.example.old;\npublic class DemoWidget { public static class Inner {} public int size() { return 1; } }\n");
    write(d, "src/com/example/user/UsesWidget.java",
        "package com.example.user;\nimport com.example.old.DemoWidget;\nimport java.util.List;\n\
         public class UsesWidget { public List<DemoWidget> widgets; public DemoWidget.Inner inner;\n\
         public int total(DemoWidget w) { return w.size() + new DemoWidget().size(); } }\n");
    let classes = d.join("classes");
    javac(&[
        "-d",
        classes.to_str().unwrap(),
        d.join("src/com/example/old/DemoWidget.java")
            .to_str()
            .unwrap(),
        d.join("src/com/example/user/UsesWidget.java")
            .to_str()
            .unwrap(),
    ]);

    // Package the classes into a JAR.
    let input = d.join("mod.jar");
    {
        let mut w = zip::ZipWriter::new(fs::File::create(&input).unwrap());
        for rel in ummp_core::checkpoint::collect_files(&classes).unwrap() {
            let name = rel.to_string_lossy().replace('\\', "/");
            w.start_file(name, zip::write::FileOptions::default())
                .unwrap();
            w.write_all(&fs::read(classes.join(&rel)).unwrap()).unwrap();
        }
        w.finish().unwrap();
    }

    let map = ClassMap::from_rules(&[RenameRule {
        from: "com.example.old.DemoWidget".into(),
        to: "com.example.demo.DemoWidget".into(),
        kind: "move".into(),
        confidence: 1.0,
        version: "1.17".into(),
    }]);
    let output = d.join("out.jar");
    let stats = remap_jar(&input, &output, &[map]).unwrap();
    assert_eq!(stats.classes_changed, 3);
    assert_eq!(stats.entries_moved, 2);

    // Code written against the NEW names compiles and runs with the remapped JAR.
    write(d, "check/Check.java",
        "import com.example.demo.DemoWidget;\nimport com.example.user.UsesWidget;\n\
         public class Check { public static void main(String[] a) {\n\
         System.out.println(new UsesWidget().total(new DemoWidget()) + \" \" + new DemoWidget.Inner().getClass().getName()); } }\n");
    let out_s = output.to_str().unwrap();
    let check = d.join("check");
    javac(&[
        "-cp",
        out_s,
        "-d",
        check.to_str().unwrap(),
        check.join("Check.java").to_str().unwrap(),
    ]);
    let sep = if cfg!(windows) { ";" } else { ":" };
    let run = Command::new("java")
        .args(["-cp", &format!("{out_s}{sep}{}", check.display()), "Check"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "2 com.example.demo.DemoWidget$Inner"
    );
}
