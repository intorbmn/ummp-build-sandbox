//! `ummp pack` end to end through the real binary: output files and exit codes.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};

fn zip_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        for (name, bytes) in entries {
            z.start_file(*name, zip::write::FileOptions::default())
                .unwrap();
            z.write_all(bytes).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

/// A real compiled class built with `javac`, or `None` without a JDK.
fn real_class() -> Option<Vec<u8>> {
    let dir = tempfile::tempdir().ok()?;
    let src = dir.path().join("Mod.java");
    std::fs::write(
        &src,
        "package p;\npublic class Mod { public int size() { return 4; } }\n",
    )
    .ok()?;
    let ok = Command::new("javac")
        .arg("-d")
        .arg(dir.path())
        .arg(&src)
        .output()
        .ok()?
        .status
        .success();
    ok.then(|| std::fs::read(dir.path().join("p/Mod.class")).ok())?
}

fn fabric_jar(id: &str, mc: &str) -> Vec<u8> {
    let desc = serde_json::json!({
        "schemaVersion": 1, "id": id, "version": "1.0.0", "name": id, "environment": "*",
        "depends": { "fabricloader": ">=0.15.0", "minecraft": mc }
    });
    let mut entries = vec![("fabric.mod.json".to_string(), desc.to_string().into_bytes())];
    if let Some(c) = real_class() {
        entries.push((format!("com/example/{id}/Mod.class"), c));
    }
    let refs: Vec<(&str, Vec<u8>)> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.clone()))
        .collect();
    zip_bytes(&refs)
}

fn evil_jar() -> Vec<u8> {
    zip_bytes(&[
        (
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"evil","version":"1.0.0","depends":{"minecraft":"1.21.1"}}"#.to_vec(),
        ),
        ("evil.properties", b"hook=https://discord.com/api/webhooks/1/abc\n".to_vec()),
    ])
}

fn pack_dir(root: &Path, jars: &[(&str, Vec<u8>)]) -> std::path::PathBuf {
    let dir = root.join("demo-pack");
    std::fs::create_dir_all(dir.join("mods")).unwrap();
    for (n, b) in jars {
        std::fs::write(dir.join("mods").join(n), b).unwrap();
    }
    dir
}

fn ummp(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(args)
        .output()
        .unwrap()
}

fn migrate(pack: &Path, out: &Path, extra: &[&str]) -> Output {
    let mut a = vec![
        "pack",
        "migrate",
        pack.to_str().unwrap(),
        "--to",
        "1.21.1",
        "--loader",
        "fabric",
        "--output",
        out.to_str().unwrap(),
    ];
    a.extend_from_slice(extra);
    ummp(&a)
}

#[test]
fn inspect_lists_mods_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let pack = pack_dir(tmp.path(), &[("a.jar", fabric_jar("a", "1.21.1"))]);
    let before = std::fs::read_dir(tmp.path()).unwrap().count();
    let o = ummp(&["pack", "inspect", pack.to_str().unwrap()]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("mods/a.jar") && out.contains("[a]"), "{out}");
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), before);
}

#[test]
fn migrate_exit_0_when_everything_fits() {
    let tmp = tempfile::tempdir().unwrap();
    let pack = pack_dir(tmp.path(), &[("a.jar", fabric_jar("a", "1.21.1"))]);
    let out = tmp.path().join("out");
    let o = migrate(&pack, &out, &[]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(out.join("instance/mods/a.jar").is_file());
    assert!(out.join("demo-pack.mrpack").is_file());
    let rep = std::fs::read_to_string(out.join("UMMP_PACK_REPORT.md")).unwrap();
    assert!(rep.contains("| a |") && rep.contains("keep"), "{rep}");
}

#[test]
fn migrate_exit_3_when_a_mod_is_left_to_the_user() {
    let tmp = tempfile::tempdir().unwrap();
    let pack = pack_dir(
        tmp.path(),
        &[
            ("a.jar", fabric_jar("a", "1.21.1")),
            ("old.jar", fabric_jar("old", "1.20.1")),
        ],
    );
    let out = tmp.path().join("out");
    let o = migrate(&pack, &out, &[]);
    assert_eq!(
        o.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    // The pack is still written, without the mod that could not be migrated.
    assert!(out.join("instance/mods/a.jar").is_file());
    assert!(!out.join("instance/mods/old.jar").exists());
    let rep = std::fs::read_to_string(out.join("UMMP_PACK_REPORT.md")).unwrap();
    assert!(rep.contains("| old |") && rep.contains("manual"), "{rep}");
}

#[test]
fn migrate_exit_2_for_high_risk_jar_unless_allowed() {
    let tmp = tempfile::tempdir().unwrap();
    let pack = pack_dir(
        tmp.path(),
        &[
            ("a.jar", fabric_jar("a", "1.21.1")),
            ("evil.jar", evil_jar()),
        ],
    );
    let out = tmp.path().join("out");
    let o = migrate(&pack, &out, &[]);
    assert_eq!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        !out.join("instance").exists(),
        "a refused pack must not be exported"
    );
    assert!(out.join("UMMP_PACK_REPORT.md").is_file());

    let out2 = tmp.path().join("out2");
    let o = migrate(&pack, &out2, &["--allow-risk"]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(out2.join("instance/mods/a.jar").is_file());
    // --allow-risk means "I reviewed it": the mod is handled like any other and the report
    // keeps the risk rating so it stays visible.
    let rep = std::fs::read_to_string(out2.join("UMMP_PACK_REPORT.md")).unwrap();
    let row = rep.lines().find(|l| l.starts_with("| evil |")).unwrap();
    assert!(row.contains("High"), "{row}");
}

#[test]
fn migrate_refuses_non_empty_output() {
    let tmp = tempfile::tempdir().unwrap();
    let pack = pack_dir(tmp.path(), &[("a.jar", fabric_jar("a", "1.21.1"))]);
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("keep.txt"), "mine").unwrap();
    let o = migrate(&pack, &out, &[]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        std::fs::read_to_string(out.join("keep.txt")).unwrap(),
        "mine"
    );
}
