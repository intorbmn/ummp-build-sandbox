//! AI key CLI contract tests. All keys are synthetic and isolated in temp directories.

use std::io::Write;
use std::process::{Command, Stdio};

const fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ummp")
}

fn assert_json_shape(actual: &serde_json::Value, expected: &serde_json::Value) {
    if let Some(sample) = expected.get("$map_value") {
        let values = actual.as_object().expect("expected a JSON object map");
        for value in values.values() {
            assert_json_shape(value, sample);
        }
        return;
    }
    match (actual, expected) {
        (serde_json::Value::Object(actual), serde_json::Value::Object(expected)) => {
            let mut actual_keys = actual.keys().collect::<Vec<_>>();
            let mut expected_keys = expected.keys().collect::<Vec<_>>();
            actual_keys.sort();
            expected_keys.sort();
            assert_eq!(actual_keys, expected_keys);
            for (key, value) in expected {
                assert_json_shape(&actual[key], value);
            }
        }
        (serde_json::Value::Array(actual), serde_json::Value::Array(expected)) => {
            if let Some(sample) = expected.first() {
                for value in actual {
                    assert_json_shape(value, sample);
                }
            }
        }
        (serde_json::Value::String(_), serde_json::Value::String(_))
        | (serde_json::Value::Number(_), serde_json::Value::Number(_))
        | (serde_json::Value::Bool(_), serde_json::Value::Bool(_))
        | (serde_json::Value::Null, serde_json::Value::Null) => {}
        _ => panic!("JSON shape mismatch: actual {actual}, expected schema {expected}"),
    }
}

#[test]
fn set_reads_stdin_and_status_only_shows_a_masked_key() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(binary())
        .args(["--json", "ai-key", "set", "--provider", "anthropic"])
        .env("UMMP_DATA_DIR", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"sk-ant-test-0000000000000000\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("sk-ant-test-0000000000000000"));
    let set: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(set["schema"], 1);
    assert_eq!(set["result"]["saved"], true);
    assert_eq!(set["result"]["masked"], "sk-ant-…0000");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/ai-key-set-result.json"
    ))
    .unwrap();
    assert_json_shape(&set["result"], &golden);

    let status = Command::new(binary())
        .args(["--json", "ai-key", "status", "--provider", "anthropic"])
        .env("UMMP_DATA_DIR", dir.path())
        .output()
        .unwrap();
    assert_eq!(status.status.code(), Some(0));
    let status_text = String::from_utf8(status.stdout).unwrap();
    assert!(!status_text.contains("sk-ant-test-0000000000000000"));
    let value: serde_json::Value = serde_json::from_str(&status_text).unwrap();
    assert_eq!(value["result"]["providers"][0]["masked"], "sk-ant-…0000");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/ai-key-status-result.json"
    ))
    .unwrap();
    assert_json_shape(&value["result"], &golden);
}

#[test]
fn a_key_cannot_be_passed_as_a_command_line_argument() {
    let output = Command::new(binary())
        .args(["ai-key", "set", "sk-ant-test-0000000000000000"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(!text.contains("sk-ant-test-0000000000000000"));
}

#[test]
fn delete_removes_only_the_selected_provider() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(binary())
        .args(["ai-key", "set", "--provider", "openai"])
        .env("UMMP_DATA_DIR", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"sk-openai-test-0000000000000000\n")
        .unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(0));

    let deleted = Command::new(binary())
        .args(["--json", "ai-key", "delete", "--provider", "openai"])
        .env("UMMP_DATA_DIR", dir.path())
        .output()
        .unwrap();
    assert_eq!(deleted.status.code(), Some(0));
    let deleted_value: serde_json::Value = serde_json::from_slice(&deleted.stdout).unwrap();
    let deleted_golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/ai-key-delete-result.json"
    ))
    .unwrap();
    assert_json_shape(&deleted_value["result"], &deleted_golden);

    let status = Command::new(binary())
        .args(["--json", "ai-key", "status", "--provider", "openai"])
        .env("UMMP_DATA_DIR", dir.path())
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(value["result"]["providers"][0]["saved"], false);
}

#[test]
fn ai_debug_resolves_a_saved_file_key_before_provider_request() {
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(binary())
        .args(["ai-key", "set", "--provider", "openai"])
        .env("UMMP_DATA_DIR", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"sk-openai-test-0000000000000000\n")
        .unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(0));

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        for request_index in 0..3 {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => return false,
                }
            };
            if stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .is_err()
            {
                return false;
            }
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return false;
                }
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = value.trim().parse::<usize>().unwrap_or(0);
                    }
                }
            }
            if content_length > 1_000_000 {
                return false;
            }
            let mut body = vec![0; content_length];
            if reader.read_exact(&mut body).is_err() {
                return false;
            }
            let content = if request_index == 1 {
                r#"{"summary":"No changes needed","risks":[],"files_to_change":[]}"#
            } else {
                r#"{"edits":[],"notes":["No changes needed"]}"#
            };
            let response = serde_json::json!({
                "choices": [{"message": {"content": content}}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1},
                "model": "test-model"
            })
            .to_string();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.len()
            );
            stream.write_all(headers.as_bytes()).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        }
        true
    });
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/loader/fabric-src");
    let log = dir.path().join("build.log");
    let snapshots = dir.path().join("snapshots");
    std::fs::write(&log, "synthetic build failure").unwrap();
    let base_url = format!("http://{address}/v1");
    let output = Command::new(binary())
        .args([
            "--json",
            "ai-debug",
            project.to_str().unwrap(),
            "--log",
            log.to_str().unwrap(),
            "--from",
            "1.20.1",
            "--to",
            "1.21.1",
            "--loader",
            "fabric",
            "--provider",
            "openai",
            "--base-url",
            &base_url,
            "--snapshots",
            snapshots.to_str().unwrap(),
        ])
        .env("UMMP_DATA_DIR", dir.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains("No openai API key"));
    assert!(!stderr.contains("No openai API key"));
    assert!(!stdout.contains("sk-openai-test-0000000000000000"));
    assert!(!stderr.contains("sk-openai-test-0000000000000000"));
    let mappings = dir.path().join("mappings.json");
    std::fs::write(&mappings, "[]").unwrap();
    let assist = Command::new(binary())
        .args([
            "--json",
            "ai-assist",
            project.to_str().unwrap(),
            "--from",
            "1.20.1",
            "--to",
            "1.21.1",
            "--loader",
            "fabric",
            "--mappings",
            mappings.to_str().unwrap(),
            "--provider",
            "openai",
            "--base-url",
            &base_url,
            "--snapshots",
            snapshots.to_str().unwrap(),
        ])
        .env("UMMP_DATA_DIR", dir.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();
    assert_eq!(
        assist.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&assist.stderr)
    );
    let assist_text = String::from_utf8_lossy(&assist.stdout);
    assert!(!assist_text.contains("sk-openai-test-0000000000000000"));
    let assist_envelope: serde_json::Value = serde_json::from_str(&assist_text).unwrap();
    assert_eq!(assist_envelope["schema"], 1);
    assert_eq!(assist_envelope["command"], "ai-assist");
    let assist_golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/ai-assist-result.json"
    ))
    .unwrap();
    assert_json_shape(&assist_envelope["result"], &assist_golden);

    assert!(
        server.join().unwrap(),
        "AI provider did not receive all requests"
    );

    let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(envelope["schema"], 1);
    assert_eq!(envelope["command"], "ai-debug");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/ai-debug-result.json"
    ))
    .unwrap();
    assert_json_shape(&envelope["result"], &golden);
}
