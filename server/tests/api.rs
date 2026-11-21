use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};
use tempfile::TempDir;
use ummp_core::{
    app::{AppServices, Settings},
    checkpoint::collect_files,
};
use ummp_server::router;

const TOKEN: &str = "test-bearer-token-with-at-least-32-bytes";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/loader/fabric-src")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    collect_files(root)
        .unwrap()
        .into_iter()
        .map(|rel| (rel.clone(), std::fs::read(root.join(rel)).unwrap()))
        .collect()
}

struct TestServer {
    _data: TempDir,
    services: Arc<AppServices>,
    base: String,
}

async fn start_server(root: &Path) -> TestServer {
    let data = tempfile::tempdir().unwrap();
    let services = Arc::new(AppServices::open(Settings::new(data.path().to_path_buf())).unwrap());
    let app = router(
        Arc::clone(&services),
        TOKEN.into(),
        vec![root.to_path_buf()],
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    TestServer {
        _data: data,
        services,
        base: format!("http://{address}"),
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build()
}

fn authorized(agent: &ureq::Agent, method: &str, url: &str) -> ureq::Request {
    match method {
        "GET" => agent.get(url),
        _ => agent.post(url),
    }
    .set("Authorization", &format!("Bearer {TOKEN}"))
    .set("Content-Type", "application/json")
}

fn status(result: Result<ureq::Response, ureq::Error>) -> u16 {
    match result {
        Ok(response) => response.status(),
        Err(ureq::Error::Status(code, _)) => code,
        Err(error) => panic!("request failed: {error}"),
    }
}

fn port_body(input: &Path, output: &Path) -> String {
    serde_json::json!({
        "input": input,
        "output": output,
        "to": "1.21.1",
        "from": "1.21.1",
        "loader": "neoforge",
        "zip": false,
        "min_confidence": 0.8,
        "kb": null,
        "mappings": null
    })
    .to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn requires_token() {
    let root = tempfile::tempdir().unwrap();
    let server = start_server(root.path()).await;
    assert_eq!(
        status(
            agent()
                .get(&format!("{}/v1/openapi.json", server.base))
                .call()
        ),
        401
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejects_path_outside_allowed_roots() {
    let root = tempfile::tempdir().unwrap();
    let server = start_server(root.path()).await;
    let response = authorized(&agent(), "POST", &format!("{}/v1/analyze", server.base))
        .send_string(r#"{"path":"/etc/passwd"}"#);
    assert_eq!(status(response), 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plan_does_not_write() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("fabric");
    copy_tree(&fixture(), &input);
    let before = snapshot(&input);
    let server = start_server(root.path()).await;
    let body = serde_json::json!({
        "path": input,
        "from": "1.21.1",
        "to": "1.21.1",
        "loader": "neoforge",
        "min_confidence": 0.8
    });
    let response = authorized(&agent(), "POST", &format!("{}/v1/plan", server.base))
        .send_string(&body.to_string());
    assert_eq!(status(response), 200);
    assert_eq!(
        snapshot(&input),
        before,
        "planning must not write to source"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn migrate_without_reviewed_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("fabric");
    copy_tree(&fixture(), &input);
    let server = start_server(root.path()).await;
    let body = serde_json::json!({
        "path": input,
        "from": "1.21.1",
        "to": "1.21.1",
        "loader": "neoforge",
        "min_confidence": 0.8,
        "reviewed": false
    });
    let response = authorized(&agent(), "POST", &format!("{}/v1/migrate", server.base))
        .send_string(&body.to_string());
    assert_eq!(status(response), 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn port_job_completes_and_sse_events_are_ordered() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("fabric");
    let output = root.path().join("ported");
    copy_tree(&fixture(), &input);
    let server = start_server(root.path()).await;

    let response = authorized(&agent(), "POST", &format!("{}/v1/port", server.base))
        .send_string(&port_body(&input, &output))
        .unwrap();
    let started: serde_json::Value =
        serde_json::from_str(&response.into_string().unwrap()).unwrap();
    let id = started["job"].as_str().unwrap();

    let response = authorized(
        &agent(),
        "GET",
        &format!("{}/v1/jobs/{id}/events", server.base),
    )
    .call()
    .unwrap();
    let mut reader = BufReader::new(response.into_reader());
    let mut stream_text = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }
        stream_text.push_str(&line);
        if line.trim() == "event: job.finished" {
            break;
        }
    }
    let started_at = stream_text
        .find("event: job.started")
        .expect("started event missing");
    let finished_at = stream_text
        .find("event: job.finished")
        .expect("finished event missing");
    assert!(started_at < finished_at, "{stream_text}");
    assert!(stream_text.contains("event: job.progress"), "{stream_text}");
    assert!(output.join("UMMP_REPORT.md").is_file());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_works() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("fabric");
    let output = root.path().join("ported");
    copy_tree(&fixture(), &input);
    let server = start_server(root.path()).await;

    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    server.services.set_step_hook(Arc::new(move |_job, step| {
        if step == "porting" {
            entered_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
        }
    }));

    let response = authorized(&agent(), "POST", &format!("{}/v1/port", server.base))
        .send_string(&port_body(&input, &output))
        .unwrap();
    let started: serde_json::Value =
        serde_json::from_str(&response.into_string().unwrap()).unwrap();
    let id = started["job"].as_str().unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();

    let response = authorized(
        &agent(),
        "POST",
        &format!("{}/v1/jobs/{id}/cancel", server.base),
    )
    .call();
    assert_eq!(status(response), 202);
    release_tx.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if server.services.job(id.parse().unwrap()).unwrap().state
                == ummp_core::jobs::JobState::Cancelled
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("job was not cancelled");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let link = root.path().join("escape");
    symlink(outside.path(), &link).unwrap();
    let server = start_server(root.path()).await;
    let response = authorized(&agent(), "POST", &format!("{}/v1/analyze", server.base))
        .send_string(&serde_json::json!({"path": link}).to_string());
    assert_eq!(status(response), 403);
}

#[test]
fn core_remains_free_of_tokio() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/Cargo.toml");
    let text = std::fs::read_to_string(manifest).unwrap();
    assert!(
        !text
            .lines()
            .any(|line| line.trim_start().starts_with("tokio")),
        "Tokio belongs only in the presentation server crate"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn body_too_large_is_413() {
    let root = tempfile::tempdir().unwrap();
    let server = start_server(root.path()).await;
    let body = format!(r#"{{"path":"{}"}}"#, "x".repeat(1024 * 1024));
    let response =
        authorized(&agent(), "POST", &format!("{}/v1/analyze", server.base)).send_string(&body);
    assert_eq!(status(response), 413);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openapi_lists_every_route() {
    let root = tempfile::tempdir().unwrap();
    let server = start_server(root.path()).await;
    assert_eq!(
        status(agent().get(&format!("{}/v1/health", server.base)).call()),
        200
    );
    let response = authorized(&agent(), "GET", &format!("{}/v1/openapi.json", server.base))
        .call()
        .unwrap();
    let openapi = response.into_string().unwrap();
    assert!(
        !openapi.contains(TOKEN),
        "server token leaked in OpenAPI response"
    );
    let spec: serde_json::Value = serde_json::from_str(&openapi).unwrap();
    let paths = spec["paths"].as_object().unwrap();
    let expected = [
        "/v1/health",
        "/v1/analyze",
        "/v1/scan",
        "/v1/plan",
        "/v1/port",
        "/v1/migrate",
        "/v1/pack",
        "/v1/build",
        "/v1/jobs/{id}",
        "/v1/jobs/{id}/events",
        "/v1/jobs/{id}/cancel",
        "/v1/sessions/{project}/rollback",
        "/v1/openapi.json",
    ];
    assert_eq!(paths.len(), expected.len());
    for route in expected {
        assert!(paths.contains_key(route), "missing {route}");
    }
}
