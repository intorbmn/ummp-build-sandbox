use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const FAKE_KEY: &str = "sk-ant-mcp-leak-test-0123456789";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/demo-fabric-mod")
}

fn start_server(root: &Path, data: &Path) -> (Child, BufReader<ChildStdout>) {
    std::fs::create_dir_all(data).unwrap();
    let config = data.join("ummp.toml");
    let allowed_root = serde_json::to_string(&root.display().to_string()).unwrap();
    std::fs::write(&config, format!("allowed_roots = [{allowed_root}]\n")).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .arg("--config")
        .arg(config)
        .arg("mcp")
        .env("UMMP_DATA_DIR", data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    (child, stdout)
}

fn send(child: &mut Child, message: &serde_json::Value) {
    let input = child.stdin.as_mut().unwrap();
    serde_json::to_writer(&mut *input, message).unwrap();
    input.write_all(b"\n").unwrap();
    input.flush().unwrap();
}

fn response(
    child: &mut Child,
    stdout: &mut BufReader<ChildStdout>,
    id: u64,
    method: &str,
    params: &serde_json::Value,
) -> serde_json::Value {
    send(
        child,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }),
    );
    loop {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        assert!(
            !line.is_empty(),
            "MCP server closed stdout before response {id}"
        );
        let value: serde_json::Value = serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("stdout contained non-JSON MCP data: {error}: {line}"));
        if value.get("id") == Some(&serde_json::json!(id)) {
            return value;
        }
    }
}

fn call_tool(
    child: &mut Child,
    stdout: &mut BufReader<ChildStdout>,
    id: u64,
    name: &str,
    arguments: &serde_json::Value,
) -> serde_json::Value {
    response(
        child,
        stdout,
        id,
        "tools/call",
        &serde_json::json!({ "name": name, "arguments": arguments }),
    )
}

fn assert_no_secret(value: &serde_json::Value, secret: &str) {
    assert!(
        !value.to_string().contains(secret),
        "MCP response leaked the stored API key"
    );
}

#[test]
fn stdio_protocol_exercises_tools_approval_and_secret_boundary() {
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let input = root.path().join("demo-fabric-mod");
    let output = root.path().join("ported");
    copy_fixture(&fixture(), &input);

    std::fs::write(
        data.path().join("credentials.json"),
        serde_json::json!({
            "schema": 1,
            "anthropic_api_key": FAKE_KEY
        })
        .to_string(),
    )
    .unwrap();

    let (mut child, mut stdout) = start_server(root.path(), data.path());
    let initialized = response(
        &mut child,
        &mut stdout,
        1,
        "initialize",
        &serde_json::json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "ummp-integration-test", "version": "0.1.0" }
        }),
    );
    assert!(initialized.get("result").is_some());
    send(
        &mut child,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    );

    let listed = response(
        &mut child,
        &mut stdout,
        2,
        "tools/list",
        &serde_json::json!({}),
    );
    let names = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    for name in [
        "ummp_analyze",
        "ummp_scan",
        "ummp_plan",
        "ummp_port",
        "ummp_build",
        "ummp_job",
        "ummp_cancel",
        "ummp_rollback",
    ] {
        assert!(names.contains(&name), "tools/list omitted {name}");
    }

    let analyze = call_tool(
        &mut child,
        &mut stdout,
        3,
        "ummp_analyze",
        &serde_json::json!({ "path": input }),
    );
    assert_no_secret(&analyze, FAKE_KEY);

    let scan = call_tool(
        &mut child,
        &mut stdout,
        4,
        "ummp_scan",
        &serde_json::json!({ "path": input }),
    );
    assert_no_secret(&scan, FAKE_KEY);

    let plan = call_tool(
        &mut child,
        &mut stdout,
        5,
        "ummp_plan",
        &serde_json::json!({ "path": input, "to": "1.21.1", "loader": "fabric" }),
    );
    assert_no_secret(&plan, FAKE_KEY);

    let before = std::fs::read(input.join("src/main/resources/fabric.mod.json")).unwrap();
    let preview = call_tool(
        &mut child,
        &mut stdout,
        6,
        "ummp_port",
        &serde_json::json!({
            "path": input,
            "to": "1.21.1",
            "loader": "fabric",
            "output": output,
            "approve": false
        }),
    );
    assert_eq!(
        preview["result"]["structuredContent"]["writes_started"],
        false
    );
    assert_eq!(
        std::fs::read(input.join("src/main/resources/fabric.mod.json")).unwrap(),
        before,
        "approve=false must not mutate the source"
    );
    assert!(!output.exists(), "approve=false must not create the output");
    assert_no_secret(&preview, FAKE_KEY);

    let started = call_tool(
        &mut child,
        &mut stdout,
        7,
        "ummp_port",
        &serde_json::json!({
            "path": input,
            "to": "1.21.1",
            "loader": "fabric",
            "output": output,
            "approve": true
        }),
    );
    assert_eq!(started["result"]["structuredContent"]["approved"], true);
    let job_id = started["result"]["structuredContent"]["job"]
        .as_str()
        .expect("approved port must return a job UUID")
        .to_owned();
    assert_no_secret(&started, FAKE_KEY);

    let build = call_tool(
        &mut child,
        &mut stdout,
        8,
        "ummp_build",
        &serde_json::json!({ "path": input, "backend": "unsupported" }),
    );
    assert_eq!(build["result"]["isError"], true);
    assert_no_secret(&build, FAKE_KEY);

    let job = call_tool(
        &mut child,
        &mut stdout,
        9,
        "ummp_job",
        &serde_json::json!({ "id": job_id }),
    );
    assert_no_secret(&job, FAKE_KEY);

    let cancel = call_tool(
        &mut child,
        &mut stdout,
        10,
        "ummp_cancel",
        &serde_json::json!({ "id": "00000000-0000-0000-0000-000000000000" }),
    );
    assert_no_secret(&cancel, FAKE_KEY);

    let rollback = call_tool(
        &mut child,
        &mut stdout,
        11,
        "ummp_rollback",
        &serde_json::json!({ "project": input, "confirm": false }),
    );
    assert_eq!(rollback["result"]["isError"], true);
    assert_no_secret(&rollback, FAKE_KEY);

    let templates = response(
        &mut child,
        &mut stdout,
        12,
        "resources/templates/list",
        &serde_json::json!({}),
    );
    assert_eq!(
        templates["result"]["resourceTemplates"][0]["uriTemplate"],
        concat!("ummp://report/{", "job}")
    );
    assert_no_secret(&templates, FAKE_KEY);

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let status = call_tool(
            &mut child,
            &mut stdout,
            13,
            "ummp_job",
            &serde_json::json!({ "id": job_id }),
        );
        assert_no_secret(&status, FAKE_KEY);
        if output.join("UMMP_REPORT.md").is_file() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "approved port job did not write its output"
        );
        thread::sleep(Duration::from_millis(100));
    }

    let report = response(
        &mut child,
        &mut stdout,
        14,
        "resources/read",
        &serde_json::json!({ "uri": format!("ummp://report/{job_id}") }),
    );
    assert_no_secret(&report, FAKE_KEY);

    child.kill().unwrap();
    let _ = child.wait();
}

fn copy_fixture(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_fixture(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
