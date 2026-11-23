//! Phase 9 `port`: source in (folder or zip) → ported source out; the input never changes.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use ummp_core::knowledge::KnowledgeBase;
use ummp_core::port::{port, PortOptions};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/loader/fabric-src")
}

fn read_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    ummp_core::checkpoint::collect_files(root)
        .unwrap()
        .into_iter()
        .map(|r| (r.clone(), std::fs::read(root.join(&r)).unwrap()))
        .collect()
}

fn kb(dir: &Path) -> KnowledgeBase {
    KnowledgeBase::open(&dir.join("kb.db")).unwrap()
}

fn opts(input: PathBuf, output: PathBuf) -> PortOptions {
    PortOptions {
        input,
        output: Some(output),
        to: "1.21.1".into(),
        loader: Some("neoforge".into()),
        from: Some("1.21.1".into()),
        zip: false,
        min_confidence: 0.8,
    }
}

fn zip_of(root: &Path, prefix: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default();
        for (rel, data) in read_tree(root) {
            let name = format!("{prefix}{}", rel.to_string_lossy().replace('\\', "/"));
            z.start_file(name, o).unwrap();
            z.write_all(&data).unwrap();
        }
        for (name, data) in extra {
            z.start_file(*name, o).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

const DESC: &str = "src/main/resources/META-INF/neoforge.mods.toml";

#[test]
fn port_folder_leaves_input_identical() {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("in");
    for (rel, data) in read_tree(&fixture()) {
        let p = input.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }
    let before = read_tree(&input);
    let out = d.path().join("out");
    let r = port(&opts(input.clone(), out.clone()), &kb(d.path())).unwrap();
    assert_eq!(r.output, out);
    assert_eq!(read_tree(&input), before, "input must not change");
    assert!(out.join(DESC).exists());
    assert!(!out.join("src/main/resources/fabric.mod.json").exists());
    let report = std::fs::read_to_string(out.join("UMMP_REPORT.md")).unwrap();
    assert!(report.contains("## Technologies"), "{report}");
    assert!(report.contains("DemoEvents.java"), "{report}");
    assert!(r.zip.is_none());
}

#[test]
fn port_zip_input() {
    let d = tempfile::tempdir().unwrap();
    let zip = d.path().join("demowidget-src.zip");
    std::fs::write(&zip, zip_of(&fixture(), "demowidget-src/", &[])).unwrap();
    let out = d.path().join("out");
    port(&opts(zip, out.clone()), &kb(d.path())).unwrap();
    assert!(
        out.join(DESC).exists(),
        "{:?}",
        read_tree(&out).keys().collect::<Vec<_>>()
    );
    assert!(out
        .join("src/main/java/com/example/demo/UmmpNeoForgeEntry.java")
        .exists());
}

#[test]
fn port_rejects_zip_slip() {
    let d = tempfile::tempdir().unwrap();
    let zip = d.path().join("evil.zip");
    std::fs::write(&zip, zip_of(&fixture(), "", &[("../evil.txt", b"x")])).unwrap();
    let out = d.path().join("out");
    let err = port(&opts(zip, out.clone()), &kb(d.path())).unwrap_err();
    assert!(err.to_string().contains("evil.txt"), "{err}");
    assert!(!d.path().join("evil.txt").exists());
    assert!(!out.exists());
}

#[test]
fn port_writes_zip() {
    let d = tempfile::tempdir().unwrap();
    let out = d.path().join("out");
    let mut options = opts(fixture(), out);
    options.zip = true;
    let result = port(&options, &kb(d.path())).unwrap();
    let zip_path = result.zip.expect("zip written");
    assert_eq!(zip_path, d.path().join("out.zip"));
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let mut text = String::new();
    archive
        .by_name(DESC)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert!(text.contains("modId = \"demowidget\""));
    assert!(archive.by_name("UMMP_REPORT.md").is_ok());
}

#[test]
fn port_refuses_non_empty_output() {
    let d = tempfile::tempdir().unwrap();
    let out = d.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("keep.txt"), "mine").unwrap();
    let err = port(&opts(fixture(), out.clone()), &kb(d.path())).unwrap_err();
    assert!(err.to_string().contains("not empty"), "{err}");
    assert_eq!(
        std::fs::read_to_string(out.join("keep.txt")).unwrap(),
        "mine"
    );
}
