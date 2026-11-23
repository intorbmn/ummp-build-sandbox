//! Phase 10 — modpack import, analysis, plan and export. Fixtures are built at test time so
//! hashes are real and descriptors use real schemas.

use serde_json::json;
use sha1::{Digest, Sha1};
use std::io::Write;
use std::path::{Path, PathBuf};
use ummp_core::metadata::Loader;
use ummp_core::modpack::{
    import, import_with, EntryKind, ImportOptions, Limits, PackError, PackFormat, Support,
};

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

fn zip_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        for (name, bytes) in entries {
            z.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(bytes).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

/// A Fabric mod JAR with a real `fabric.mod.json` and a binary-looking class entry.
fn fabric_jar(id: &str, mc: &str, deps: &[(&str, &str)]) -> Vec<u8> {
    let mut depends = serde_json::Map::new();
    depends.insert("fabricloader".into(), json!(">=0.15.0"));
    depends.insert("minecraft".into(), json!(mc));
    for (k, v) in deps {
        depends.insert((*k).into(), json!(v));
    }
    let desc = json!({
        "schemaVersion": 1, "id": id, "version": "1.0.0", "name": id,
        "environment": "*", "depends": depends
    });
    let class_name = format!("com/example/{id}/Mod.class");
    let mut entries: Vec<(&str, Vec<u8>)> =
        vec![("fabric.mod.json", desc.to_string().into_bytes())];
    if let Some(c) = real_class() {
        entries.push((class_name.as_str(), c));
    }
    zip_bytes(&entries)
}

/// A real compiled class (binary `.class`), built once with `javac`; `None` without a JDK.
fn real_class() -> Option<Vec<u8>> {
    static CLASS: std::sync::OnceLock<Option<Vec<u8>>> = std::sync::OnceLock::new();
    CLASS
        .get_or_init(|| {
            let dir = tempfile::tempdir().ok()?;
            let src = dir.path().join("Mod.java");
            std::fs::write(
                &src,
                "package p;\npublic class Mod { public int size() { return 4; } }\n",
            )
            .ok()?;
            let ok = std::process::Command::new("javac")
                .arg("-d")
                .arg(dir.path())
                .arg(&src)
                .output()
                .ok()?
                .status
                .success();
            ok.then(|| std::fs::read(dir.path().join("p/Mod.class")).ok())?
        })
        .clone()
}

fn forge_jar(id: &str, mc: &str) -> Vec<u8> {
    let toml = format!(
        "modLoader=\"javafml\"\nloaderVersion=\"[47,)\"\nlicense=\"MIT\"\n\
         [[mods]]\nmodId=\"{id}\"\nversion=\"1.0.0\"\ndisplayName=\"{id}\"\n\
         [[dependencies.{id}]]\nmodId=\"minecraft\"\nmandatory=true\nversionRange=\"[{mc}]\"\n\
         ordering=\"NONE\"\nside=\"BOTH\"\n"
    );
    zip_bytes(&[("META-INF/mods.toml", toml.into_bytes())])
}

fn sha1_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha1::digest(bytes)
        .iter()
        .fold(String::new(), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

fn hex(len: usize) -> String {
    "ab".repeat(len / 2)
}

fn index(files: &serde_json::Value) -> Vec<u8> {
    json!({
        "formatVersion": 1, "game": "minecraft", "versionId": "1.0.0", "name": "Demo Pack",
        "summary": "synthetic",
        "files": files,
        "dependencies": { "minecraft": "1.20.1", "fabric-loader": "0.15.11" }
    })
    .to_string()
    .into_bytes()
}

fn remote_file(path: &str) -> serde_json::Value {
    json!({
        "path": path,
        "hashes": { "sha1": hex(40), "sha512": hex(128) },
        "env": { "client": "required", "server": "optional" },
        "downloads": ["https://cdn.modrinth.com/data/demo/versions/v1/demowidget-1.0.0.jar"],
        "fileSize": 12345
    })
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, bytes).unwrap();
    p
}

fn all_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(all_files(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 10.A Import
// ---------------------------------------------------------------------------

#[test]
fn imports_mrpack_files_and_overrides() {
    let tmp = tempfile::tempdir().unwrap();
    let extra = fabric_jar("extra", "1.20.1", &[]);
    let pack = write(
        tmp.path(),
        "demo.mrpack",
        &zip_bytes(&[
            (
                "modrinth.index.json",
                index(&json!([remote_file("mods/demowidget-1.0.0.jar")])),
            ),
            ("overrides/config/demowidget.toml", b"size = 4\n".to_vec()),
            ("overrides/mods/extra.jar", extra.clone()),
        ]),
    );
    let work = tmp.path().join("work");
    let p = import(&pack, &work).unwrap();

    assert_eq!(p.format, PackFormat::Mrpack);
    assert_eq!(p.name, "Demo Pack");
    assert_eq!(p.mc.as_deref(), Some("1.20.1"));
    assert_eq!(p.loader, Some(Loader::Fabric));
    assert_eq!(p.loader_version.as_deref(), Some("0.15.11"));

    assert_eq!(p.entries.len(), 2);
    let remote = &p.entries[0];
    assert_eq!(remote.path, "mods/demowidget-1.0.0.jar");
    assert_eq!(remote.kind, EntryKind::Mod);
    assert!(remote.local.is_none());
    assert_eq!(remote.env.client, Support::Required);
    assert_eq!(remote.env.server, Support::Optional);
    assert_eq!(remote.downloads.len(), 1);
    assert_eq!(remote.sha1.as_deref(), Some(hex(40).as_str()));
    assert_eq!(remote.size, Some(12345));

    let local = &p.entries[1];
    assert_eq!(local.path, "mods/extra.jar");
    assert_eq!(local.sha1.as_deref(), Some(sha1_hex(&extra).as_str()));
    assert!(local.local.as_ref().unwrap().exists());

    assert!(p
        .overrides
        .contains(&PathBuf::from("config/demowidget.toml")));
    assert_eq!(
        std::fs::read_to_string(p.root.join("config/demowidget.toml")).unwrap(),
        "size = 4\n"
    );
}

#[test]
fn rejects_mrpack_with_path_traversal() {
    for bad in [
        "../evil.jar",
        "/abs.jar",
        "C:\\x.jar",
        "mods/../../evil.jar",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let pack = write(
            tmp.path(),
            "bad.mrpack",
            &zip_bytes(&[("modrinth.index.json", index(&json!([remote_file(bad)])))]),
        );
        let work = tmp.path().join("work");
        let err = import(&pack, &work).unwrap_err();
        assert!(matches!(err, PackError::UnsafePath(_)), "{bad}: {err:?}");
        // Nothing outside the work dir was created.
        let outside: Vec<_> = all_files(tmp.path())
            .into_iter()
            .filter(|f| !f.starts_with(&work) && f != &pack)
            .collect();
        assert!(outside.is_empty(), "{bad}: {outside:?}");
    }
}

#[test]
fn rejects_zip_slip_in_overrides() {
    let tmp = tempfile::tempdir().unwrap();
    let pack = write(
        tmp.path(),
        "slip.mrpack",
        &zip_bytes(&[
            ("modrinth.index.json", index(&json!([]))),
            ("overrides/../../evil.txt", b"x".to_vec()),
        ]),
    );
    let work = tmp.path().join("a").join("work");
    let err = import(&pack, &work).unwrap_err();
    assert!(matches!(err, PackError::UnsafePath(_)), "{err:?}");
    assert!(!tmp.path().join("evil.txt").exists());
    assert!(!tmp.path().join("a").join("evil.txt").exists());
}

#[test]
fn rejects_mrpack_hash_mismatch() {
    // The zip bundles the file at its listed path, but the bytes do not match the hash.
    let tmp = tempfile::tempdir().unwrap();
    let pack = write(
        tmp.path(),
        "h.mrpack",
        &zip_bytes(&[
            (
                "modrinth.index.json",
                index(&json!([remote_file("mods/x.jar")])),
            ),
            ("mods/x.jar", b"not the declared bytes".to_vec()),
        ]),
    );
    let err = import(&pack, &tmp.path().join("work")).unwrap_err();
    assert!(matches!(err, PackError::HashMismatch { .. }), "{err:?}");

    // A malformed hash is refused even when nothing is bundled.
    let mut f = remote_file("mods/y.jar");
    f["hashes"]["sha1"] = json!("abc");
    let pack = write(
        tmp.path(),
        "m.mrpack",
        &zip_bytes(&[("modrinth.index.json", index(&json!([f])))]),
    );
    let err = import(&pack, &tmp.path().join("work2")).unwrap_err();
    assert!(matches!(err, PackError::Invalid(_)), "{err:?}");
}

#[test]
fn caps_archive_entry_size() {
    let tmp = tempfile::tempdir().unwrap();
    let pack = write(
        tmp.path(),
        "big.mrpack",
        &zip_bytes(&[
            ("modrinth.index.json", index(&json!([]))),
            ("overrides/big.bin", vec![0u8; 2 * 1024 * 1024]),
        ]),
    );
    let opts = ImportOptions {
        limits: Limits {
            max_entry_bytes: 1024 * 1024,
            ..Limits::default()
        },
        ..ImportOptions::default()
    };
    let err = import_with(&pack, &tmp.path().join("work"), &opts).unwrap_err();
    assert!(matches!(err, PackError::LimitExceeded(_)), "{err:?}");
    // The default limits accept the same pack.
    assert!(import(&pack, &tmp.path().join("work2")).is_ok());
}

#[test]
fn imports_curseforge_manifest_as_unresolved() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = json!({
        "minecraft": { "version": "1.20.1", "modLoaders": [{ "id": "forge-47.2.0", "primary": true }] },
        "manifestType": "minecraftModpack", "manifestVersion": 1, "name": "Demo Pack",
        "version": "1.0.0", "author": "demo",
        "files": [{ "projectID": 123, "fileID": 456, "required": true }],
        "overrides": "overrides"
    });
    let pack = write(
        tmp.path(),
        "demo-curseforge.zip",
        &zip_bytes(&[
            ("manifest.json", manifest.to_string().into_bytes()),
            ("overrides/config/demo.cfg", b"S:size=4\n".to_vec()),
        ]),
    );
    let p = import(&pack, &tmp.path().join("work")).unwrap();
    assert_eq!(p.format, PackFormat::CurseForge);
    assert_eq!(p.name, "Demo Pack");
    assert_eq!(p.mc.as_deref(), Some("1.20.1"));
    assert_eq!(p.loader, Some(Loader::Forge));
    assert_eq!(p.loader_version.as_deref(), Some("47.2.0"));
    assert_eq!(p.entries.len(), 1);
    let e = &p.entries[0];
    assert_eq!(e.curseforge, Some((123, 456)));
    assert!(e.local.is_none() && e.sha1.is_none() && e.downloads.is_empty());
    assert!(p.overrides.contains(&PathBuf::from("config/demo.cfg")));
}

#[test]
fn imports_prism_instance_components() {
    let tmp = tempfile::tempdir().unwrap();
    let inst = tmp.path().join("demo-prism");
    write(
        &inst,
        "mmc-pack.json",
        json!({ "formatVersion": 1, "components": [
            { "uid": "net.minecraft", "version": "1.20.1" },
            { "uid": "net.fabricmc.fabric-loader", "version": "0.15.11" } ] })
        .to_string()
        .as_bytes(),
    );
    write(&inst, "instance.cfg", b"[General]\nname=Demo Instance\n");
    write(
        &inst,
        "minecraft/mods/a.jar",
        &fabric_jar("a", "1.20.1", &[]),
    );
    write(&inst, "minecraft/config/a.toml", b"x = 1\n");
    let p = import(&inst, &tmp.path().join("work")).unwrap();
    assert_eq!(p.format, PackFormat::Prism);
    assert_eq!(p.name, "Demo Instance");
    assert_eq!(p.mc.as_deref(), Some("1.20.1"));
    assert_eq!(p.loader, Some(Loader::Fabric));
    assert_eq!(p.loader_version.as_deref(), Some("0.15.11"));
    assert_eq!(p.entries.len(), 1);
    assert_eq!(p.entries[0].path, "mods/a.jar");
    assert!(p.entries[0].local.is_some());
    assert!(p.overrides.contains(&PathBuf::from("config/a.toml")));
}

#[test]
fn imports_plain_folder_majority_loader() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("pack");
    write(&dir, "mods/a.jar", &fabric_jar("a", "1.20.1", &[]));
    write(&dir, "mods/b.jar", &fabric_jar("b", "1.20.1", &[]));
    write(&dir, "mods/c.jar", &forge_jar("c", "1.20.1"));
    write(&dir, "config/a.toml", b"x = 1\n");
    write(&dir, "kubejs/server_scripts/x.js", b"// script\n");

    let p = import(&dir, &tmp.path().join("work")).unwrap();
    assert_eq!(p.format, PackFormat::Folder);
    assert_eq!(p.loader, Some(Loader::Fabric));
    assert_eq!(p.mc.as_deref(), Some("1.20.1"));
    assert_eq!(p.entries.len(), 3);
    assert!(p
        .overrides
        .contains(&PathBuf::from("kubejs/server_scripts/x.js")));

    // --from-loader / --from-mc win over detection.
    let opts = ImportOptions {
        from_loader: Some(Loader::Forge),
        from_mc: Some("1.19.2".into()),
        ..ImportOptions::default()
    };
    let p = import_with(&dir, &tmp.path().join("work2"), &opts).unwrap();
    assert_eq!(p.loader, Some(Loader::Forge));
    assert_eq!(p.mc.as_deref(), Some("1.19.2"));
}

// ---------------------------------------------------------------------------
// 10.B Analysis and graph
// ---------------------------------------------------------------------------

use ummp_core::modpack::analyze::{analyze_pack, gate, AnalyzeOptions};
use ummp_core::modpack::graph::{PackGraph, PackIssue};
use ummp_core::modpack::Pack;
use ummp_core::scan::Severity;

/// A plain-folder pack holding the given JARs under `mods/`.
fn folder_pack(tmp: &Path, jars: &[(&str, Vec<u8>)]) -> Pack {
    let dir = tmp.join("pack");
    for (name, bytes) in jars {
        write(&dir, &format!("mods/{name}"), bytes);
    }
    import(&dir, &tmp.join("work")).unwrap()
}

fn analyzed(tmp: &Path, jars: &[(&str, Vec<u8>)]) -> Pack {
    let mut p = folder_pack(tmp, jars);
    analyze_pack(&mut p, &AnalyzeOptions { threads: 2 }).unwrap();
    p
}

#[test]
fn graph_reports_missing_required_dependency() {
    let tmp = tempfile::tempdir().unwrap();
    let p = analyzed(
        tmp.path(),
        &[("a.jar", fabric_jar("a", "1.20.1", &[("libb", ">=1.0")]))],
    );
    let g = PackGraph::build(&p.entries);
    assert!(
        g.issues.iter().any(|i| matches!(i,
            PackIssue::MissingDependency { mod_id, requires } if mod_id == "a" && requires == "libb")),
        "{:?}",
        g.issues
    );
    // The built-ins (minecraft, fabricloader) are never reported.
    assert!(!g.issues.iter().any(|i| matches!(i,
        PackIssue::MissingDependency { requires, .. } if requires == "minecraft" || requires == "fabricloader")));

    // With the dependency present, nothing is missing.
    let tmp2 = tempfile::tempdir().unwrap();
    let p = analyzed(
        tmp2.path(),
        &[
            ("a.jar", fabric_jar("a", "1.20.1", &[("libb", ">=1.0")])),
            ("libb.jar", fabric_jar("libb", "1.20.1", &[])),
        ],
    );
    assert!(PackGraph::build(&p.entries).issues.is_empty());
}

#[test]
fn graph_reports_duplicate_mod_id() {
    let tmp = tempfile::tempdir().unwrap();
    let p = analyzed(
        tmp.path(),
        &[
            ("dup-1.jar", fabric_jar("dup", "1.20.1", &[])),
            ("dup-2.jar", fabric_jar("dup", "1.20.1", &[])),
        ],
    );
    let g = PackGraph::build(&p.entries);
    assert!(g.issues.iter().any(|i| matches!(i,
        PackIssue::DuplicateId { mod_id, paths } if mod_id == "dup" && paths.len() == 2)));
}

#[test]
fn graph_reports_cycle_without_hanging() {
    let tmp = tempfile::tempdir().unwrap();
    let p = analyzed(
        tmp.path(),
        &[
            ("a.jar", fabric_jar("a", "1.20.1", &[("b", "*")])),
            ("b.jar", fabric_jar("b", "1.20.1", &[("a", "*")])),
            ("c.jar", fabric_jar("c", "1.20.1", &[("a", "*")])),
        ],
    );
    let g = PackGraph::build(&p.entries);
    let cycle = g
        .issues
        .iter()
        .find_map(|i| match i {
            PackIssue::Cycle { mods } => Some(mods.clone()),
            _ => None,
        })
        .expect("a cycle");
    assert_eq!(cycle, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn graph_reports_side_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let mut p = analyzed(
        tmp.path(),
        &[
            ("a.jar", fabric_jar("a", "1.20.1", &[("lib", "*")])),
            ("lib.jar", fabric_jar("lib", "1.20.1", &[])),
        ],
    );
    for e in &mut p.entries {
        if e.path == "mods/lib.jar" {
            e.env.server = Support::Unsupported; // client-only library
        }
    }
    let g = PackGraph::build(&p.entries);
    assert!(
        g.issues.iter().any(|i| matches!(i,
        PackIssue::SideMismatch { mod_id, requires, side }
            if mod_id == "a" && requires == "lib" && *side == "server")),
        "{:?}",
        g.issues
    );
}

#[test]
fn pack_with_high_risk_jar_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let evil = zip_bytes(&[
        (
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"evil","version":"1.0.0","depends":{"minecraft":"1.20.1"}}"#
                .to_vec(),
        ),
        (
            "evil.properties",
            b"hook=https://discord.com/api/webhooks/1/abc\n".to_vec(),
        ),
    ]);
    let p = analyzed(
        tmp.path(),
        &[
            ("good.jar", fabric_jar("good", "1.20.1", &[])),
            ("evil.jar", evil),
        ],
    );
    let evil = p
        .entries
        .iter()
        .find(|e| e.path == "mods/evil.jar")
        .unwrap();
    assert_eq!(evil.risk, Some(Severity::High));
    let good = p
        .entries
        .iter()
        .find(|e| e.path == "mods/good.jar")
        .unwrap();
    assert_eq!(good.risk, None);

    let err = gate(&p, false).unwrap_err();
    assert!(
        matches!(err, PackError::Refused(ref m) if m.contains("evil.jar")),
        "{err:?}"
    );
    assert!(gate(&p, true).is_ok());
}

fn many_jars(n: usize) -> Vec<(String, Vec<u8>)> {
    (0..n)
        .map(|i| {
            let deps: Vec<(String, String)> = if i > 0 {
                vec![(format!("m{}", i - 1), "*".into())]
            } else {
                vec![]
            };
            let deps_ref: Vec<(&str, &str)> =
                deps.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
            (
                format!("m{i:03}.jar"),
                fabric_jar(&format!("m{i}"), "1.20.1", &deps_ref),
            )
        })
        .collect()
}

#[test]
fn parallel_analysis_matches_sequential() {
    let jars = many_jars(24);
    let refs: Vec<(&str, Vec<u8>)> = jars.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    let tmp = tempfile::tempdir().unwrap();
    let mut one = folder_pack(tmp.path(), &refs);
    let mut many = import(&tmp.path().join("pack"), &tmp.path().join("w2")).unwrap();
    analyze_pack(&mut one, &AnalyzeOptions { threads: 1 }).unwrap();
    analyze_pack(&mut many, &AnalyzeOptions { threads: 8 }).unwrap();
    assert_eq!(
        serde_json::to_string(&one.entries).unwrap(),
        serde_json::to_string(&many.entries).unwrap()
    );
    assert!(one.entries.iter().all(|e| e.mod_info.is_some()));
}

#[test]
fn analysis_is_deterministic() {
    let jars = many_jars(12);
    let refs: Vec<(&str, Vec<u8>)> = jars.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    let tmp = tempfile::tempdir().unwrap();
    let mut a = folder_pack(tmp.path(), &refs);
    let mut b = import(&tmp.path().join("pack"), &tmp.path().join("w2")).unwrap();
    analyze_pack(&mut a, &AnalyzeOptions { threads: 4 }).unwrap();
    analyze_pack(&mut b, &AnalyzeOptions { threads: 4 }).unwrap();
    let ga = PackGraph::build(&a.entries);
    let gb = PackGraph::build(&b.entries);
    assert_eq!(
        serde_json::to_string(&a.entries).unwrap(),
        serde_json::to_string(&b.entries).unwrap()
    );
    assert_eq!(format!("{:?}", ga.issues), format!("{:?}", gb.issues));
}

// ---------------------------------------------------------------------------
// 10.C Plan with a fake index
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use ummp_core::modpack::config::{PackRules, ReplacementRule};
use ummp_core::modpack::plan::{
    plan, ModIndex, PackAction, PackPlan, PlanInput, RemoteVersion, Side, Target,
};

#[derive(Default)]
struct FakeIndex {
    versions: HashMap<String, Vec<RemoteVersion>>, // key "project|mc|loader"
    hashes: HashMap<String, RemoteVersion>,
    calls: AtomicUsize,
}

impl ModIndex for FakeIndex {
    fn versions(
        &self,
        project: &str,
        mc: &str,
        loader: &Loader,
    ) -> Result<Vec<RemoteVersion>, PackError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let l = match loader {
            Loader::Fabric => "fabric",
            _ => "other",
        };
        Ok(self
            .versions
            .get(&format!("{project}|{mc}|{l}"))
            .cloned()
            .unwrap_or_default())
    }
    fn by_hash(&self, sha1: &str) -> Result<Option<RemoteVersion>, PackError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.hashes.get(sha1).cloned())
    }
}

/// Fails the test when anything asks it for data.
struct PanicIndex;
impl ModIndex for PanicIndex {
    fn versions(&self, _: &str, _: &str, _: &Loader) -> Result<Vec<RemoteVersion>, PackError> {
        panic!("offline mode must not call the index");
    }
    fn by_hash(&self, _: &str) -> Result<Option<RemoteVersion>, PackError> {
        panic!("offline mode must not call the index");
    }
}

fn remote(project: &str, version: &str) -> RemoteVersion {
    RemoteVersion {
        project: project.into(),
        version: version.into(),
        file_name: format!("{project}-{version}.jar"),
        url: format!(
            "https://cdn.modrinth.com/data/{project}/versions/{version}/{project}-{version}.jar"
        ),
        // Distinct, non-repeating values so a swapped hash or a truncated one shows up.
        sha1: sha1_hex(format!("{project}{version}1").as_bytes()),
        sha512: format!(
            "{}{}{}{}",
            sha1_hex(format!("{project}{version}2").as_bytes()),
            sha1_hex(format!("{project}{version}3").as_bytes()),
            sha1_hex(format!("{project}{version}4").as_bytes()),
            sha1_hex(format!("{project}{version}5").as_bytes())
        )[..128]
            .to_string(),
        size: 100,
        required_deps: vec![],
        env: ummp_core::modpack::Env::default(),
    }
}

fn target(mc: &str) -> Target {
    Target {
        mc: mc.into(),
        loader: Loader::Fabric,
        loader_version: Some("0.16.2".into()),
    }
}

fn input<'a>(mc: &str, rules: &'a [ReplacementRule]) -> PlanInput<'a> {
    PlanInput {
        target: target(mc),
        online: false,
        sources: None,
        side: Side::Both,
        allow_risk: false,
        replacements: rules,
    }
}

fn action<'a>(p: &'a PackPlan, path: &str) -> &'a PackAction {
    &p.items.iter().find(|i| i.path == path).unwrap().action
}

#[test]
fn plan_keeps_mod_that_supports_target() {
    let tmp = tempfile::tempdir().unwrap();
    let p = analyzed(
        tmp.path(),
        &[
            ("a.jar", fabric_jar("a", "1.21.1", &[])),
            ("old.jar", fabric_jar("old", "1.20.1", &[])),
        ],
    );
    let plan = plan(&p, &input("1.21.1", &[]), &PanicIndex).unwrap();
    assert!(matches!(action(&plan, "mods/a.jar"), PackAction::Keep));
    // A mod built for another version is not kept just because it exists.
    assert!(matches!(
        action(&plan, "mods/old.jar"),
        PackAction::Manual { .. }
    ));
    assert!(plan.items.iter().all(|i| !i.reason.is_empty()));
}

#[test]
fn plan_updates_when_index_has_target_version() {
    let tmp = tempfile::tempdir().unwrap();
    let p = analyzed(tmp.path(), &[("a.jar", fabric_jar("a", "1.20.1", &[]))]);
    let mut idx = FakeIndex::default();
    idx.versions.insert(
        "a|1.21.1|fabric".into(),
        vec![remote("a", "2.0.0"), remote("a", "1.9.0")],
    );
    let mut inp = input("1.21.1", &[]);
    inp.online = true;
    let plan = plan(&p, &inp, &idx).unwrap();
    match action(&plan, "mods/a.jar") {
        PackAction::Update(rv) => assert_eq!(rv.version, "2.0.0"), // newest first
        other => panic!("{other:?}"),
    }
    // No version for the target: left to the user, with the reason.
    let mut inp = input("1.99", &[]);
    inp.online = true;
    let plan = ummp_core::modpack::plan::plan(&p, &inp, &idx).unwrap();
    assert!(matches!(
        action(&plan, "mods/a.jar"),
        PackAction::Manual { .. }
    ));
}

#[test]
fn plan_ports_when_source_supplied() {
    let tmp = tempfile::tempdir().unwrap();
    let p = analyzed(
        tmp.path(),
        &[("dw.jar", fabric_jar("demowidget", "1.20.1", &[]))],
    );
    let sources = tmp.path().join("sources");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/loader/fabric-src");
    copy_dir(&fixture, &sources.join("demowidget"));
    let mut inp = input("1.21.1", &[]);
    inp.sources = Some(&sources);
    let plan = plan(&p, &inp, &PanicIndex).unwrap();
    match action(&plan, "mods/dw.jar") {
        PackAction::Port { source } => assert!(source.ends_with("demowidget")),
        other => panic!("{other:?}"),
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

#[test]
fn plan_replaces_by_rule() {
    let tmp = tempfile::tempdir().unwrap();
    let p = analyzed(
        tmp.path(),
        &[("a.jar", fabric_jar("demowidget", "1.20.1", &[]))],
    );
    let rules = vec![ReplacementRule {
        from: "demowidget".into(),
        to: "demogadget".into(),
        loader: Some("fabric".into()),
        note: "synthetic".into(),
    }];
    let plan1 = plan(&p, &input("1.21.1", &rules), &PanicIndex).unwrap();
    assert!(matches!(
        action(&plan1, "mods/a.jar"),
        PackAction::Replace { with, resolved: None } if with == "demogadget"
    ));
    // Online, the replacement is resolved to a downloadable version.
    let mut idx = FakeIndex::default();
    idx.versions.insert(
        "demogadget|1.21.1|fabric".into(),
        vec![remote("demogadget", "1.0.0")],
    );
    let mut inp = input("1.21.1", &rules);
    inp.online = true;
    let plan2 = ummp_core::modpack::plan::plan(&p, &inp, &idx).unwrap();
    assert!(matches!(
        action(&plan2, "mods/a.jar"),
        PackAction::Replace {
            resolved: Some(_),
            ..
        }
    ));
    // A rule for another loader does not apply.
    let mut other = rules.clone();
    other[0].loader = Some("forge".into());
    let plan3 = ummp_core::modpack::plan::plan(&p, &input("1.21.1", &other), &PanicIndex).unwrap();
    assert!(matches!(
        action(&plan3, "mods/a.jar"),
        PackAction::Manual { .. }
    ));
}

#[test]
fn plan_marks_manual_with_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let p = analyzed(tmp.path(), &[("a.jar", fabric_jar("a", "1.20.1", &[]))]);
    let plan = plan(&p, &input("1.21.1", &[]), &PanicIndex).unwrap();
    match action(&plan, "mods/a.jar") {
        PackAction::Manual { reason } => {
            assert!(reason.contains("no version for 1.21.1/fabric"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn plan_rechecks_dependencies_after_decisions() {
    let tmp = tempfile::tempdir().unwrap();
    let p = analyzed(
        tmp.path(),
        &[
            ("c.jar", fabric_jar("c", "1.21.1", &[("a", "*")])),
            ("a.jar", fabric_jar("a", "1.21.1", &[("lib", "*")])),
            ("lib.jar", fabric_jar("lib", "1.20.1", &[])),
        ],
    );
    let plan = plan(&p, &input("1.21.1", &[]), &PanicIndex).unwrap();
    assert!(matches!(
        action(&plan, "mods/lib.jar"),
        PackAction::Manual { .. }
    ));
    // a would be kept on its own, but its dependency is left to the user; c follows a.
    for (path, missing) in [("mods/a.jar", "lib"), ("mods/c.jar", "a")] {
        match action(&plan, path) {
            PackAction::Manual { reason } => assert!(
                reason.contains(&format!("missing {missing}")),
                "{path}: {reason}"
            ),
            other => panic!("{path}: {other:?}"),
        }
    }
}

#[test]
fn offline_mode_never_calls_index() {
    let tmp = tempfile::tempdir().unwrap();
    let p = analyzed(
        tmp.path(),
        &[
            ("a.jar", fabric_jar("a", "1.20.1", &[])),
            ("b.jar", fabric_jar("b", "1.21.1", &[])),
        ],
    );
    // PanicIndex panics on any call; online = false must not reach it.
    let _ = plan(&p, &input("1.21.1", &[]), &PanicIndex).unwrap();
    // And with online = true a counting index is used.
    let idx = FakeIndex::default();
    let mut inp = input("1.21.1", &[]);
    inp.online = true;
    let _ = ummp_core::modpack::plan::plan(&p, &inp, &idx).unwrap();
    assert!(idx.calls.load(Ordering::SeqCst) > 0);
}

#[test]
fn plan_removes_high_risk_and_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let evil = zip_bytes(&[
        (
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"evil","version":"1.0.0","depends":{"minecraft":"1.21.1"}}"#.to_vec(),
        ),
        ("x.properties", b"h=https://discord.com/api/webhooks/1/abc\n".to_vec()),
    ]);
    let p = analyzed(tmp.path(), &[("evil.jar", evil)]);
    let pl = plan(&p, &input("1.21.1", &[]), &PanicIndex).unwrap();
    assert!(
        matches!(action(&pl, "mods/evil.jar"), PackAction::Remove { reason } if reason.contains("malware risk"))
    );
    assert!(pl.refused.is_some());
    let mut inp = input("1.21.1", &[]);
    inp.allow_risk = true;
    let pl = ummp_core::modpack::plan::plan(&p, &inp, &PanicIndex).unwrap();
    assert!(pl.refused.is_none());
    assert!(matches!(action(&pl, "mods/evil.jar"), PackAction::Keep));
}

#[test]
fn plan_keeps_remote_entry_when_target_equals_pack() {
    let tmp = tempfile::tempdir().unwrap();
    let pack = write(
        tmp.path(),
        "demo.mrpack",
        &zip_bytes(&[(
            "modrinth.index.json",
            index(&json!([remote_file("mods/demowidget-1.0.0.jar")])),
        )]),
    );
    let p = import(&pack, &tmp.path().join("work")).unwrap();
    // Same mc + loader as the pack: the listed file is still valid.
    let same = plan(&p, &input("1.20.1", &[]), &PanicIndex).unwrap();
    assert!(matches!(
        action(&same, "mods/demowidget-1.0.0.jar"),
        PackAction::Keep
    ));
    // Another version: nothing is known about it offline.
    let other = ummp_core::modpack::plan::plan(&p, &input("1.21.1", &[]), &PanicIndex).unwrap();
    assert!(matches!(
        action(&other, "mods/demowidget-1.0.0.jar"),
        PackAction::Manual { .. }
    ));
}

#[test]
fn plan_removes_wrong_side_mod() {
    let tmp = tempfile::tempdir().unwrap();
    let mut p = analyzed(tmp.path(), &[("a.jar", fabric_jar("a", "1.20.1", &[]))]);
    p.entries[0].env.server = Support::Unsupported;
    let mut inp = input("1.21.1", &[]);
    inp.side = Side::Server;
    let pl = plan(&p, &inp, &PanicIndex).unwrap();
    assert!(
        matches!(action(&pl, "mods/a.jar"), PackAction::Remove { reason } if reason.contains("wrong side"))
    );
    let _ = PackRules::builtin().unwrap();
}

// ---------------------------------------------------------------------------
// 10.E Config migration
// ---------------------------------------------------------------------------

use std::collections::BTreeSet;
use ummp_core::migration::MigrationError;
use ummp_core::modpack::config::{copy_overrides, migrate_config, ConfigRule, KeyRename};

fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut v: Vec<_> = all_files(root)
        .into_iter()
        .map(|p| {
            (
                p.strip_prefix(root).unwrap().to_path_buf(),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect();
    v.sort();
    v
}

fn rule(mod_id: &str, file: &str, from: &str, to: &str) -> ConfigRule {
    ConfigRule {
        mod_id: mod_id.into(),
        since_mc: "1.21".into(),
        file: file.into(),
        rename_file: None,
        rename_key: vec![KeyRename {
            from: from.into(),
            to: to.into(),
        }],
    }
}

/// An instance folder with a config tree, imported and copied to a fresh output folder.
fn config_out(tmp: &Path, files: &[(&str, &str)]) -> PathBuf {
    let dir = tmp.join("pack");
    for (rel, body) in files {
        write(&dir, rel, body.as_bytes());
    }
    write(&dir, "mods/a.jar", &fabric_jar("demowidget", "1.20.1", &[]));
    let p = import(&dir, &tmp.join("work")).unwrap();
    let out = tmp.join("out");
    copy_overrides(&p, &out).unwrap();
    out
}

fn ids(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn config_rule_renames_key() {
    let tmp = tempfile::tempdir().unwrap();
    let toml = "# widget settings\nsize = 4\n[sub]\nsize_x = 1\n  size =  9 # indented\n";
    let out = config_out(
        tmp.path(),
        &[
            ("config/demowidget.toml", toml),
            (
                "config/demowidget.json",
                "{\"size\": 4, \"nested\": {\"size\": 5}, \"sizes\": 1}",
            ),
            (
                "config/demowidget.properties",
                "size=4\n# size=ignored\nsize.x=1\n",
            ),
            (
                "config/demowidget.cfg",
                "general {\n    I:size=4\n    S:\"size\"=x\n    I:sizes=2\n}\n",
            ),
        ],
    );
    let rules = vec![
        rule("demowidget", "config/demowidget.toml", "size", "capacity"),
        rule("demowidget", "config/demowidget.json", "size", "capacity"),
        rule(
            "demowidget",
            "config/demowidget.properties",
            "size",
            "capacity",
        ),
        rule("demowidget", "config/demowidget.cfg", "size", "capacity"),
    ];
    let snaps = tempfile::tempdir().unwrap();
    let rep = migrate_config(
        &out,
        &rules,
        &ids(&["demowidget"]),
        "1.20.1",
        "1.21.1",
        snaps.path(),
    )
    .unwrap();
    assert_eq!(rep.changes.len(), 4);
    assert!(rep.checkpoint.is_some());

    let read = |f: &str| std::fs::read_to_string(out.join(f)).unwrap();
    assert_eq!(
        read("config/demowidget.toml"),
        "# widget settings\ncapacity = 4\n[sub]\nsize_x = 1\n  capacity =  9 # indented\n"
    );
    assert_eq!(
        read("config/demowidget.json"),
        "{\"capacity\": 4, \"nested\": {\"capacity\": 5}, \"sizes\": 1}"
    );
    assert_eq!(
        read("config/demowidget.properties"),
        "capacity=4\n# size=ignored\nsize.x=1\n"
    );
    assert_eq!(
        read("config/demowidget.cfg"),
        "general {\n    I:capacity=4\n    S:\"capacity\"=x\n    I:sizes=2\n}\n"
    );

    // A rule does not apply when the mod is absent, or the target is before the change.
    let tmp2 = tempfile::tempdir().unwrap();
    let out2 = config_out(tmp2.path(), &[("config/demowidget.toml", "size = 4\n")]);
    let snaps2 = tempfile::tempdir().unwrap();
    let before = tree(&out2);
    let r = migrate_config(
        &out2,
        &rules[..1],
        &ids(&["other"]),
        "1.20.1",
        "1.21.1",
        snaps2.path(),
    )
    .unwrap();
    assert!(r.changes.is_empty() && r.checkpoint.is_none());
    let r = migrate_config(
        &out2,
        &rules[..1],
        &ids(&["demowidget"]),
        "1.20.1",
        "1.20.4",
        snaps2.path(),
    )
    .unwrap();
    assert!(r.changes.is_empty());
    let r = migrate_config(
        &out2,
        &rules[..1],
        &ids(&["demowidget"]),
        "1.21.1",
        "1.21.4",
        snaps2.path(),
    )
    .unwrap();
    assert!(r.changes.is_empty(), "already past the change");
    assert_eq!(tree(&out2), before);
}

#[test]
fn config_without_rule_is_copied_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let out = config_out(
        tmp.path(),
        &[
            ("config/a.toml", "size = 4\n"),
            ("kubejs/server_scripts/x.js", "// script\r\nlet a = 1;\r\n"),
            ("options.txt", "lang:en_us\n"),
        ],
    );
    let snaps = tempfile::tempdir().unwrap();
    let rep = migrate_config(
        &out,
        &[],
        &ids(&["demowidget"]),
        "1.20.1",
        "1.21.1",
        snaps.path(),
    )
    .unwrap();
    assert!(rep.changes.is_empty() && rep.moves.is_empty());
    let t = tree(&out);
    assert_eq!(t.len(), 3);
    assert!(t
        .iter()
        .any(|(p, b)| p == Path::new("kubejs/server_scripts/x.js")
            && b == b"// script\r\nlet a = 1;\r\n"));
}

#[test]
fn config_rollback_restores_original() {
    let tmp = tempfile::tempdir().unwrap();
    let out = config_out(
        tmp.path(),
        &[
            ("config/demowidget.toml", "size = 4\n"),
            ("config/blocker", "i am a file"),
        ],
    );
    let before = tree(&out);
    // The edit is written first; the move then fails because `config/blocker` is a file.
    let mut r = rule("demowidget", "config/demowidget.toml", "size", "capacity");
    r.rename_file = Some("config/blocker/demowidget.toml".into());
    let snaps = tempfile::tempdir().unwrap();
    let err = migrate_config(
        &out,
        &[r],
        &ids(&["demowidget"]),
        "1.20.1",
        "1.21.1",
        snaps.path(),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            PackError::Migration(MigrationError::ApplyFailed {
                rolled_back: true,
                ..
            })
        ),
        "{err:?}"
    );
    assert_eq!(tree(&out), before);
}

// ---------------------------------------------------------------------------
// 10.F export and report
// ---------------------------------------------------------------------------

use ummp_core::modpack::formats::{build_output, write_folder, write_mrpack, OutSource};
use ummp_core::modpack::report::PackReport;

/// Pack with one mod that fits 1.21.1 (kept) and one that the index can update.
fn export_fixture(tmp: &Path) -> (Pack, PackPlan) {
    let p = analyzed(
        tmp,
        &[
            ("a.jar", fabric_jar("a", "1.21.1", &[])),
            ("b.jar", fabric_jar("b", "1.20.1", &[])),
            ("c.jar", fabric_jar("c", "1.20.1", &[])),
        ],
    );
    let mut idx = FakeIndex::default();
    idx.versions
        .insert("b|1.21.1|fabric".into(), vec![remote("b", "2.0.0")]);
    let mut inp = input("1.21.1", &[]);
    inp.online = true;
    let pl = plan(&p, &inp, &idx).unwrap();
    (p, pl)
}

#[test]
fn export_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let (p, pl) = export_fixture(tmp.path());
    let out = build_output(&p, &pl, "0.16.2").unwrap();
    // c has no target version: it is left out of the pack and listed in the report.
    let paths: Vec<_> = out.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["mods/a.jar", "mods/b-2.0.0.jar"]);

    let inst = tmp.path().join("inst");
    write_folder(&out, &inst).unwrap();
    assert!(inst.join("mods/a.jar").exists());
    assert!(!inst.join("mods/c.jar").exists());
    let file = tmp.path().join("o/pack.mrpack");
    write_mrpack(&out, &inst, &file).unwrap();

    let back = import(&file, &tmp.path().join("w2")).unwrap();
    assert_eq!(back.format, PackFormat::Mrpack);
    assert_eq!(back.mc.as_deref(), Some("1.21.1"));
    assert_eq!(back.loader, Some(Loader::Fabric));
    assert_eq!(back.loader_version.as_deref(), Some("0.16.2"));
    let mut got: Vec<_> = back.entries.iter().map(|e| e.path.clone()).collect();
    got.sort();
    assert_eq!(got, ["mods/a.jar", "mods/b-2.0.0.jar"]);
    let b = back
        .entries
        .iter()
        .find(|e| e.path == "mods/b-2.0.0.jar")
        .unwrap();
    let rv = remote("b", "2.0.0");
    assert_eq!(b.sha1.as_deref(), Some(rv.sha1.as_str()));
    assert_eq!(b.sha512.as_deref(), Some(rv.sha512.as_str()));
    assert_eq!(b.downloads, [rv.url]);
    assert!(
        b.local.is_none(),
        "an updated mod is listed by URL, never bundled"
    );
    let a = back
        .entries
        .iter()
        .find(|e| e.path == "mods/a.jar")
        .unwrap();
    assert!(a.local.is_some());
}

#[test]
fn exported_index_has_required_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let (pack, pl) = export_fixture(tmp.path());
    let out = build_output(&pack, &pl, "0.16.2").unwrap();
    let inst = tmp.path().join("inst");
    write_folder(&out, &inst).unwrap();
    let file = tmp.path().join("pack.mrpack");
    write_mrpack(&out, &inst, &file).unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&file).unwrap()).unwrap();
    let mut text = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("modrinth.index.json").unwrap(),
        &mut text,
    )
    .unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["formatVersion"], 1);
    assert_eq!(doc["game"], "minecraft");
    assert_eq!(doc["dependencies"]["minecraft"], "1.21.1");
    assert_eq!(doc["dependencies"]["fabric-loader"], "0.16.2");
    let f = &doc["files"][0];
    assert_eq!(f["path"], "mods/b-2.0.0.jar");
    assert_eq!(f["hashes"]["sha1"].as_str().unwrap().len(), 40);
    assert_eq!(f["hashes"]["sha512"].as_str().unwrap().len(), 128);
    assert!(f["downloads"][0].as_str().unwrap().starts_with("https://"));
    assert!(f["fileSize"].as_u64().unwrap() > 0);
    assert!(f["env"]["client"].is_string() && f["env"]["server"].is_string());
    archive.by_name("overrides/mods/a.jar").unwrap();
    assert!(matches!(out.entries[0].source, OutSource::Local(_)));
}

#[test]
fn report_lists_every_mod_once_with_action_and_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let (p, pl) = export_fixture(tmp.path());
    let mut r = PackReport::build(&p, &pl, &[], &["w1".to_string()]);
    assert_eq!(r.rows.len(), 3);
    let md = r.to_markdown();
    for (id, act) in [("a", "keep"), ("b", "update"), ("c", "manual")] {
        let rows: Vec<_> = md
            .lines()
            .filter(|l| l.starts_with(&format!("| {id} |")))
            .collect();
        assert_eq!(rows.len(), 1, "{id} must appear exactly once:\n{md}");
        assert!(rows[0].contains(&format!("| {act} |")), "{}", rows[0]);
        let cols: Vec<_> = rows[0].split(" | ").collect();
        assert!(!cols[4].trim().is_empty(), "reason missing: {}", rows[0]);
    }
    assert!(md.contains("- manual: 1"));
    assert!(md.contains("- w1"));
    r.rows[0].reason = "x|y".into();
    assert!(
        r.to_markdown().contains("x\\|y"),
        "pipes in cells are escaped"
    );
}
