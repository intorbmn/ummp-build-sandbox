//! Phase 10.D — the Modrinth client against a local fake server (no network).
//! The response bodies follow the real API shape with synthetic names.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use ummp_ai_providers::ModrinthIndex;
use ummp_core::metadata::Loader;
use ummp_core::modpack::plan::ModIndex;
use ummp_core::modpack::PackError;

/// Serves `responses` (status, body) one per connection and records each request head.
fn fake_server(responses: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for (status, body) in responses {
            let (mut s, _) = listener.accept().unwrap();
            let mut head = Vec::new();
            let mut b = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if s.read(&mut b).unwrap() == 0 {
                    break;
                }
                head.push(b[0]);
            }
            log.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&head).into_owned());
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            s.write_all(reply.as_bytes()).unwrap();
        }
    });
    (base, seen)
}

fn version_json(project: &str, number: &str) -> String {
    format!(
        r#"{{"id":"v-{number}","project_id":"{project}","name":"Demo {number}","version_number":"{number}",
        "game_versions":["1.21.1"],"loaders":["fabric"],"date_published":"2026-01-01T00:00:00Z",
        "files":[{{"hashes":{{"sha1":"{}","sha512":"{}"}},
                   "url":"https://cdn.modrinth.com/data/{project}/versions/v-{number}/{project}-{number}.jar",
                   "filename":"{project}-{number}.jar","primary":true,"size":4321}}],
        "dependencies":[{{"project_id":"libdemo","dependency_type":"required"}},
                        {{"project_id":"extra","dependency_type":"optional"}}]}}"#,
        "ab".repeat(20),
        "cd".repeat(64),
    )
}

fn client(base: &str) -> ModrinthIndex {
    ModrinthIndex::with_base(base).with_retry_delay(Duration::ZERO)
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn by_hash_parses_version_file() {
    let (base, seen) = fake_server(vec![(200, version_json("demoproj", "1.2.0"))]);
    let v = client(&base).by_hash(&"ab".repeat(20)).unwrap().unwrap();
    assert_eq!(v.project, "demoproj");
    assert_eq!(v.version, "1.2.0");
    assert_eq!(v.file_name, "demoproj-1.2.0.jar");
    assert_eq!(v.size, 4321);
    assert_eq!(v.sha1, "ab".repeat(20));
    assert_eq!(v.required_deps, vec!["libdemo".to_string()]);
    let req = seen.lock().unwrap()[0].clone();
    assert!(
        req.starts_with(&format!(
            "GET /v2/version_file/{}?algorithm=sha1 ",
            "ab".repeat(20)
        )),
        "{req}"
    );
}

#[test]
fn by_hash_unknown_file_is_none() {
    let (base, _) = fake_server(vec![(404, r#"{"error":"not_found"}"#.into())]);
    assert!(client(&base).by_hash(&"00".repeat(20)).unwrap().is_none());
}

#[test]
fn versions_request_has_loader_and_game_version_params() {
    let body = format!(
        "[{},{}]",
        version_json("demoproj", "2.0.0"),
        version_json("demoproj", "1.0.0")
    );
    let (base, seen) = fake_server(vec![(200, body)]);
    let vs = client(&base)
        .versions("demoproj", "1.21.1", &Loader::Fabric)
        .unwrap();
    assert_eq!(vs.len(), 2);
    assert_eq!(vs[0].version, "2.0.0"); // order of the response is kept: newest first
    let req = seen.lock().unwrap()[0].clone();
    let line = decode(req.lines().next().unwrap());
    assert!(
        line.starts_with("GET /v2/project/demoproj/version?"),
        "{line}"
    );
    assert!(line.contains(r#"loaders=["fabric"]"#), "{line}");
    assert!(line.contains(r#"game_versions=["1.21.1"]"#), "{line}");
}

#[test]
fn project_id_cannot_inject_into_the_path() {
    let (base, seen) = fake_server(vec![(200, "[]".into())]);
    let _ = client(&base)
        .versions("../../x?y=1", "1.21.1", &Loader::Fabric)
        .unwrap();
    let req = seen.lock().unwrap()[0].clone();
    let line = req.lines().next().unwrap();
    assert!(
        line.starts_with("GET /v2/project/..%2F..%2Fx%3Fy%3D1/version?"),
        "{line}"
    );
}

#[test]
fn sends_user_agent() {
    let (base, seen) = fake_server(vec![(200, "[]".into())]);
    let _ = client(&base)
        .versions("demoproj", "1.21.1", &Loader::Fabric)
        .unwrap();
    let req = seen.lock().unwrap()[0].to_ascii_lowercase();
    assert!(
        req.contains("user-agent: intorbmn/universal-minecraft-mod-porter/"),
        "{req}"
    );
}

#[test]
fn http_429_retried_once_then_error() {
    let (base, seen) = fake_server(vec![(429, "{}".into()), (429, "{}".into())]);
    let err = client(&base)
        .versions("demoproj", "1.21.1", &Loader::Fabric)
        .unwrap_err();
    assert!(
        matches!(err, PackError::Index(ref m) if m.contains("429")),
        "{err:?}"
    );
    assert_eq!(seen.lock().unwrap().len(), 2, "exactly one retry");

    // A 429 followed by success is fine.
    let (base, seen) = fake_server(vec![(429, "{}".into()), (200, "[]".into())]);
    assert!(client(&base)
        .versions("demoproj", "1.21.1", &Loader::Fabric)
        .unwrap()
        .is_empty());
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn invalid_json_is_error_not_panic() {
    for body in ["not json", r#"{"unexpected":true}"#, r#"[{"id":1}]"#] {
        let (base, _) = fake_server(vec![(200, body.into())]);
        let err = client(&base)
            .versions("demoproj", "1.21.1", &Loader::Fabric)
            .unwrap_err();
        assert!(matches!(err, PackError::Index(_)), "{body}: {err:?}");
    }
    let (base, _) = fake_server(vec![(200, "garbage".into())]);
    assert!(matches!(
        client(&base).by_hash(&"ab".repeat(20)).unwrap_err(),
        PackError::Index(_)
    ));
}
