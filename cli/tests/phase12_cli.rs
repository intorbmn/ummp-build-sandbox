//! Phase 12 CLI contract tests against the real binary.

use std::process::{Command, Output, Stdio};

fn ummp(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(args)
        .output()
        .expect("run ummp")
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
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

fn assert_json_shape(actual: &serde_json::Value, expected: &serde_json::Value) {
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
#[should_panic(expected = "JSON shape mismatch")]
fn json_shape_rejects_null_where_object_is_expected() {
    assert_json_shape(
        &serde_json::Value::Null,
        &serde_json::json!({ "field": "" }),
    );
}

#[test]
#[should_panic(expected = "JSON shape mismatch")]
fn json_shape_rejects_string_where_number_is_expected() {
    assert_json_shape(&serde_json::json!("1"), &serde_json::json!(1));
}

#[test]
#[should_panic(expected = "JSON shape mismatch")]
fn json_shape_rejects_wrong_array_item_type() {
    assert_json_shape(
        &serde_json::json!([null]),
        &serde_json::json!([{"field": ""}]),
    );
}

#[test]
fn explicit_toml_config_is_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom.toml");
    let log = dir.path().join("build.log");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/loader/fabric-src");
    std::fs::write(
        &path,
        r#"ai_provider = "openai"
openai_base_url = "http://localhost:11434/v1"
"#,
    )
    .unwrap();
    std::fs::write(&log, "synthetic build failure").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--config",
            path.to_str().unwrap(),
            "--json",
            "ai-debug",
            fixture.to_str().unwrap(),
            "--log",
            log.to_str().unwrap(),
            "--from",
            "1.20.1",
            "--to",
            "1.21.1",
            "--loader",
            "fabric",
        ])
        .env("UMMP_DATA_DIR", data.path())
        .env_remove("UMMP_AI_PROVIDER")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["error"]["kind"], "refused");
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No openai API key"));
}

#[test]
fn config_rejects_api_key_fields_without_echoing_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ummp.toml");
    std::fs::write(&path, "anthropic_api_key = \"synthetic-secret-value\"").unwrap();
    let output = ummp(&["--config", path.to_str().unwrap(), "adapters"]);
    assert_eq!(output.status.code(), Some(5));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("secret-like fields"));
    assert!(!stderr.contains("synthetic-secret-value"));
}

#[test]
fn missing_explicit_config_is_invalid_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.toml");
    let output = ummp(&["--config", path.to_str().unwrap(), "adapters"]);
    assert_eq!(output.status.code(), Some(5));
}

#[test]
fn global_json_flag_wraps_scan_view() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["--json", "scan", dir.path().to_str().unwrap()])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["command"], "scan");
    assert!(value["result"]["findings"].is_array());
}

#[test]
fn global_json_flag_works_after_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["scan", dir.path().to_str().unwrap(), "--json"])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["command"], "scan");
}

#[test]
fn json_errors_have_a_stable_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["--json", "scan", missing.to_str().unwrap()])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["error"]["kind"], "invalid_input");
    assert!(value["error"]["message"].is_string());
}

#[test]
fn invalid_arguments_use_exit_code_five() {
    let output = ummp(&["--unknown-option"]);
    assert_eq!(output.status.code(), Some(5));
}

#[test]
fn build_rejects_non_project_input_with_code_five() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing-project");
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["build", missing.to_str().unwrap()])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
}

#[test]
fn help_is_successful() {
    let output = ummp(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn completions_are_generated_for_supported_shells() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let output = ummp(&["completions", shell]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let script = String::from_utf8_lossy(&output.stdout);
        assert!(script.contains("ummp"), "{shell} completion was empty");
        assert!(script.contains("analyze"), "{shell} lacks subcommands");
        let marker = match shell {
            "bash" => "complete -F",
            "zsh" => "#compdef ummp",
            "fish" => "complete -c ummp",
            "powershell" => "Register-ArgumentCompleter",
            _ => unreachable!(),
        };
        assert!(
            script.contains(marker),
            "{shell} completion registration missing"
        );
    }
}

#[test]
fn bash_completion_can_be_sourced_and_registered() {
    let dir = tempfile::tempdir().unwrap();
    let output = ummp(&["completions", "bash"]);
    assert_eq!(output.status.code(), Some(0));
    let script = dir.path().join("ummp.bash");
    std::fs::write(&script, output.stdout).unwrap();

    let completion = Command::new("bash")
        .arg("-c")
        .arg(r#"source "$1"; complete -p ummp"#)
        .arg("ummp-completion")
        .arg(&script)
        .output()
        .expect("bash must be available for the completion smoke test");
    assert_eq!(
        completion.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&completion.stderr)
    );
    let registration = String::from_utf8_lossy(&completion.stdout);
    assert!(registration.contains("_ummp"), "{registration}");
}
#[test]
fn analyze_json_matches_golden_view_shape() {
    let data = tempfile::tempdir().unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/loader/fabric-src");
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["--json", "analyze", fixture.to_str().unwrap()])
        .env("UMMP_DATA_DIR", data.path())
        .current_dir(data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/analyze-result.json"
    ))
    .unwrap();
    assert_eq!(actual["schema"], 1);
    assert_eq!(actual["command"], "analyze");
    assert_json_shape(&actual["result"], &golden);
}

#[test]
fn scan_json_has_stable_versioned_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["scan", dir.path().to_str().unwrap(), "--json"])
        .env("UMMP_DATA_DIR", data.path())
        .current_dir(data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/scan-envelope.json"
    ))
    .unwrap();
    assert_json_shape(&actual, &golden);
    assert_eq!(actual["schema"], 1);
    assert_eq!(actual["command"], "scan");
}

#[test]
fn adapters_json_has_stable_versioned_result() {
    let data = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["--json", "adapters"])
        .env("UMMP_DATA_DIR", data.path())
        .current_dir(data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["command"], "adapters");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/adapters-result.json"
    ))
    .unwrap();
    assert_json_shape(&value["result"], &golden);
    let adapters = value["result"]["adapters"].as_array().unwrap();
    assert!(!adapters.is_empty());
}

#[test]
fn migrate_json_matches_plan_view_golden_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let mappings = dir.path().join("mappings.json");
    std::fs::write(&mappings, "[]").unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/loader/fabric-src");
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "migrate",
            fixture.to_str().unwrap(),
            "--from",
            "1.20.1",
            "--to",
            "1.21.1",
            "--loader",
            "fabric",
            "--mappings",
            mappings.to_str().unwrap(),
        ])
        .env("UMMP_DATA_DIR", data.path())
        .current_dir(data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/migrate-plan-result.json"
    ))
    .unwrap();
    assert_eq!(actual["schema"], 1);
    assert_eq!(actual["command"], "migrate");
    assert_json_shape(&actual["result"], &golden);
}

#[test]
fn port_json_missing_input_uses_invalid_input_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing-mod");
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "port",
            missing.to_str().unwrap(),
            "--to",
            "1.21.1",
        ])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["error"]["kind"], "invalid_input");
}

#[test]
fn rollback_json_missing_project_uses_invalid_input_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing-project");
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["--json", "rollback", missing.to_str().unwrap()])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["error"]["kind"], "invalid_input");
}

#[test]
fn build_json_missing_project_uses_invalid_input_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing-project");
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["--json", "build", missing.to_str().unwrap()])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["error"]["kind"], "invalid_input");
}

#[test]
fn port_json_matches_golden_report_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/loader/fabric-src");
    let output_dir = dir.path().join("ported");
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "port",
            source.to_str().unwrap(),
            "--from",
            "1.20.1",
            "--to",
            "1.21.1",
            "--loader",
            "neoforge",
            "--output",
            output_dir.to_str().unwrap(),
        ])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["schema"], 1);
    assert_eq!(envelope["command"], "port");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/port-result.json"
    ))
    .unwrap();
    assert_json_shape(&envelope["result"], &golden);
}

#[test]
fn remap_jar_json_matches_golden_result_shape() {
    use std::io::Write;
    use ummp_core::knowledge::{KnowledgeBase, Mapping};

    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let kb_path = dir.path().join("mappings.db");
    let kb = KnowledgeBase::open(&kb_path).unwrap();
    kb.insert_mapping(&Mapping {
        version: "1.17".into(),
        loader: "forge".into(),
        from_api: "example.old.Widget".into(),
        to_api: "example.new.Widget".into(),
        kind: "move".into(),
        source: "phase12-test".into(),
        confidence: 0.99,
    })
    .unwrap();
    drop(kb);

    let input = dir.path().join("input.jar");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&input).unwrap());
    archive
        .start_file(
            "META-INF/MANIFEST.MF",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    archive.write_all(b"Manifest-Version: 1.0\n\n").unwrap();
    archive.finish().unwrap();
    let output_path = dir.path().join("output.jar");

    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "remap-jar",
            input.to_str().unwrap(),
            output_path.to_str().unwrap(),
            "--from",
            "1.16.5",
            "--to",
            "1.17.1",
            "--loader",
            "forge",
            "--kb",
            kb_path.to_str().unwrap(),
        ])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_path.is_file());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["schema"], 1);
    assert_eq!(envelope["command"], "remap-jar");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/remap-jar-result.json"
    ))
    .unwrap();
    assert_json_shape(&envelope["result"], &golden);
}

#[test]
fn rollback_json_matches_golden_result_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/loader/fabric-src");
    let project = dir.path().join("project");
    copy_tree(&source, &project);
    let mappings = dir.path().join("mappings.json");
    let snapshots = dir.path().join("snapshots");
    std::fs::write(&mappings, "[]").unwrap();

    let migrate = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "migrate",
            project.to_str().unwrap(),
            "--from",
            "1.21.1",
            "--to",
            "1.21.1",
            "--loader",
            "neoforge",
            "--mappings",
            mappings.to_str().unwrap(),
            "--apply",
            "--snapshots",
            snapshots.to_str().unwrap(),
        ])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(
        migrate.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&migrate.stdout),
        String::from_utf8_lossy(&migrate.stderr)
    );

    let rollback = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "rollback",
            project.to_str().unwrap(),
            "--snapshots",
            snapshots.to_str().unwrap(),
        ])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(
        rollback.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&rollback.stdout),
        String::from_utf8_lossy(&rollback.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&rollback.stdout).unwrap();
    assert_eq!(envelope["schema"], 1);
    assert_eq!(envelope["command"], "rollback");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/rollback-result.json"
    ))
    .unwrap();
    assert_json_shape(&envelope["result"], &golden);
    assert_eq!(envelope["result"]["restored"], true);
}

#[test]
fn migrate_json_missing_input_uses_invalid_input_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing-mod");
    let mappings = dir.path().join("mappings.json");
    std::fs::write(&mappings, "[]").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "migrate",
            missing.to_str().unwrap(),
            "--from",
            "1.20.1",
            "--to",
            "1.21.1",
            "--loader",
            "neoforge",
            "--mappings",
            mappings.to_str().unwrap(),
        ])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(5),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["error"]["kind"], "invalid_input");
}

#[test]
fn pack_inspect_json_returns_a_structured_pack_result() {
    let pack = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args(["--json", "pack", "inspect", pack.path().to_str().unwrap()])
        .env("UMMP_DATA_DIR", data.path())
        .current_dir(data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["command"], "pack");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/pack-inspect-result.json"
    ))
    .unwrap();
    assert_json_shape(&value["result"], &golden);
    assert_eq!(value["result"]["pack"]["format"], "folder");
    assert!(value["result"]["pack"]["entries"].is_array());
    assert!(value["result"]["issues"].is_array());
}

#[test]
fn ai_debug_missing_key_is_a_json_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let log = dir.path().join("build.log");
    std::fs::write(&log, "synthetic build failure").unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/loader/fabric-src");
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "ai-debug",
            fixture.to_str().unwrap(),
            "--log",
            log.to_str().unwrap(),
            "--from",
            "1.20.1",
            "--to",
            "1.21.1",
            "--loader",
            "fabric",
        ])
        .env("UMMP_DATA_DIR", data.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["error"]["kind"], "refused");
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("ai-key set"));
}

#[test]
fn cli_and_environment_provider_values_override_toml() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("ummp.toml");
    let log = dir.path().join("build.log");
    std::fs::write(&log, "synthetic failure").unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/loader/fabric-src");

    let run = |config_provider: &str, env_provider: &str, cli_provider: Option<&str>| {
        std::fs::write(&config, format!("ai_provider = \"{config_provider}\"")).unwrap();
        let data = tempfile::tempdir().unwrap();
        let mut args = vec![
            "--config",
            config.to_str().unwrap(),
            "--json",
            "ai-debug",
            fixture.to_str().unwrap(),
            "--log",
            log.to_str().unwrap(),
            "--from",
            "1.20.1",
            "--to",
            "1.21.1",
            "--loader",
            "fabric",
        ];
        if let Some(provider) = cli_provider {
            args.extend(["--provider", provider]);
        }
        Command::new(env!("CARGO_BIN_EXE_ummp"))
            .args(args)
            .env("UMMP_DATA_DIR", data.path())
            .env("UMMP_AI_PROVIDER", env_provider)
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .output()
            .unwrap()
    };

    // CLI flag differs from both TOML and the environment.
    let cli = run("openai", "openai", Some("anthropic"));
    assert_eq!(cli.status.code(), Some(2));
    let cli_error: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert!(cli_error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No anthropic API key"));

    // Environment differs from TOML when no explicit CLI value is present.
    let env = run("anthropic", "openai", None);
    assert_eq!(env.status.code(), Some(2));
    let env_error: serde_json::Value = serde_json::from_slice(&env.stdout).unwrap();
    assert!(env_error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No openai API key"));
}

#[cfg(unix)]
#[test]
fn local_build_success_matches_golden_result_shape() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let project = dir.path().join("working-gradle");
    let workdir = dir.path().join("build-work");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("settings.gradle"),
        "rootProject.name = 'working'",
    )
    .unwrap();
    std::fs::write(project.join("build.gradle"), "tasks.register('build')").unwrap();
    let wrapper = project.join("gradlew");
    std::fs::write(&wrapper, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "build",
            project.to_str().unwrap(),
            "--local",
            "--yes",
            "--i-accept-risk",
            "--stages",
            "build",
            "--timeout",
            "10",
            "--workdir",
            workdir.to_str().unwrap(),
        ])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["schema"], 1);
    assert_eq!(envelope["command"], "build");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/golden/cli/build-result.json"
    ))
    .unwrap();
    assert_json_shape(&envelope["result"], &golden);
    assert_eq!(envelope["result"]["passed"], true);
}

#[cfg(unix)]
#[test]
fn local_build_failure_uses_exit_code_four_and_json_result() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let project = dir.path().join("broken-gradle");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("settings.gradle"),
        "rootProject.name = 'broken'",
    )
    .unwrap();
    std::fs::write(
        project.join("build.gradle"),
        "throw new GradleException('intentional build failure')",
    )
    .unwrap();
    let wrapper = project.join("gradlew");
    std::fs::write(&wrapper, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "build",
            project.to_str().unwrap(),
            "--local",
            "--yes",
            "--i-accept-risk",
            "--stages",
            "build",
            "--timeout",
            "10",
        ])
        .env("UMMP_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(4),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["command"], "build");
    assert!(value["result"].is_object());
}

#[test]
fn invalid_openai_base_url_does_not_save_cli_key() {
    let data = tempfile::tempdir().unwrap();
    let key_input = data.path().join("key.txt");
    std::fs::write(&key_input, "sk-openai-test-0000000000000000\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ummp"))
        .args([
            "--json",
            "ai-key",
            "set",
            "--provider",
            "openai",
            "--base-url",
            "file:///tmp/invalid",
        ])
        .env("UMMP_DATA_DIR", data.path())
        .stdin(Stdio::from(std::fs::File::open(&key_input).unwrap()))
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(5),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["kind"], "invalid_input");
    let credentials = data.path().join("credentials.json");
    assert!(
        !credentials.exists(),
        "invalid base URL must not persist the API key"
    );
    for entry in std::fs::read_dir(data.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.path() == key_input {
            continue;
        }
        if entry.file_type().unwrap().is_file() {
            let contents = std::fs::read(entry.path()).unwrap();
            assert!(
                !String::from_utf8_lossy(&contents).contains("sk-openai-test-0000000000000000"),
                "API key leaked into {}",
                entry.path().display()
            );
        }
    }
}
