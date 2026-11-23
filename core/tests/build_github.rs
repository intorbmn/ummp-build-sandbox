//! Phase 8 GitHub backend against fake `gh` and `git` scripts (no network).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use ummp_core::app::Secret;
use ummp_core::build::github::{workflow_yaml, GithubBackend};
use ummp_core::build::logs::ErrorKind;
use ummp_core::build::{BuildBackend, BuildRequest, Stage, StageStatus};

const ALL: [Stage; 3] = [Stage::Build, Stage::Test, Stage::Server];

// Fake executable scripts share the OS exec path; serialize them to avoid Linux ETXTBSY races.
static FAKE_TOOLS_LOCK: Mutex<()> = Mutex::new(());

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Fake tools that log their argv to `calls` and serve `artifact` on `run download`.
fn fakes(dir: &Path, artifact: &Path, auth_ok: bool) -> (PathBuf, PathBuf) {
    let calls = dir.join("calls");
    let gh = dir.join("gh");
    let git = dir.join("git");
    let auth = i32::from(!auth_ok);
    script(
        &gh,
        &format!(
            "echo \"gh $*\" >> '{calls}'\n\
             echo \"GH_TOKEN=${{GH_TOKEN:-}}\" >> '{calls}'\n\
             case \"$1 $2\" in\n\
             'auth status') exit 1 ;;\n\
             'api repos/demo-owner/ummp-build-sandbox/actions/runs/42/jobs') echo '{{\"jobs\":[{{\"id\":7}}]}}' ;;\n\
             'api repos/demo-owner/ummp-build-sandbox/actions/jobs/7/logs') \
               echo '2026-11-19T07:32:25Z echo \"UMMP-STAGES $(jq -c . ummp-out/stages.json)\"'; \
               echo '2026-11-19T07:32:39Z /home/runner/work/x/x/src/main/java/com/example/demo/DemoWidget.java:7: error: cannot find symbol'; \
               echo '2026-11-19T07:32:40Z UMMP-STAGES [{{\"stage\":\"build\",\"status\":\"passed\"}},{{\"stage\":\"test\",\"status\":\"passed\"}},{{\"stage\":\"server\",\"status\":\"failed\"}}]' ;;\n\
             'api repos/'*) exit {auth} ;;\n\
             'run list') echo '[{{\"databaseId\":42,\"status\":\"completed\",\"conclusion\":\"success\"}}]' ;;\n\
             'run download') [ -d '{art}' ] || exit 1; while [ $# -gt 0 ]; do [ \"$1\" = -D ] && D=$2; shift; done; mkdir -p \"$D\"; cp -r '{art}'/. \"$D\" ;;\n\
             esac\n",
            calls = calls.display(),
            art = artifact.display()
        ),
    );
    script(&git, &format!("echo \"git $*\" >> '{}'\n", calls.display()));
    (gh, git)
}

fn backend(dir: &Path, gh: PathBuf, git: PathBuf) -> GithubBackend {
    GithubBackend {
        repo: "demo-owner/ummp-build-sandbox".into(),
        gh,
        git,
        poll: Duration::from_millis(10),
        workdir: dir.join("work"),
        token: None,
    }
}

fn request() -> BuildRequest {
    BuildRequest {
        project: root().join("tests/fixtures/build-demo"),
        mod_name: "build-demo".into(),
        stages: ALL.to_vec(),
        java: Some(21),
        timeout: Duration::from_mins(30),
    }
}

#[test]
fn workflow_matches_golden() {
    let golden =
        std::fs::read_to_string(root().join("tests/fixtures/golden/ummp-build.yml")).unwrap();
    assert_eq!(workflow_yaml(21, &ALL, 1800), golden);
}

#[test]
fn fake_gh_round_trip() {
    let _guard = FAKE_TOOLS_LOCK.lock().unwrap();
    let d = tempfile::tempdir().unwrap();
    let art = d.path().join("art");
    std::fs::create_dir_all(art.join("test-results/test")).unwrap();
    std::fs::write(
        art.join("stages.json"),
        r#"[{"stage":"build","status":"passed"},{"stage":"test","status":"passed"},{"stage":"server","status":"passed"}]"#,
    )
    .unwrap();
    std::fs::write(art.join("build.log"), "BUILD SUCCESSFUL\n").unwrap();
    std::fs::write(
        art.join("test-results/test/TEST-demo.xml"),
        r#"<?xml version="1.0"?><testsuite name="demo" tests="3" failures="0" errors="0"></testsuite>"#,
    )
    .unwrap();
    let (gh, git) = fakes(d.path(), &art, true);
    let o = backend(d.path(), gh, git).run(&request()).unwrap();

    let statuses: Vec<_> = o.stages.iter().map(|s| s.status).collect();
    assert_eq!(statuses, vec![StageStatus::Passed; 3]);
    assert_eq!(o.tests, Some((3, 3)));
    let calls = std::fs::read_to_string(d.path().join("calls")).unwrap();
    assert!(calls.contains("push"), "{calls}");
    assert!(
        calls.contains("https://github.com/demo-owner/ummp-build-sandbox.git"),
        "{calls}"
    );
    assert!(
        calls.contains("HEAD:refs/heads/ummp/build-demo-"),
        "{calls}"
    );
    assert!(calls.contains("gh run download 42"), "{calls}");
    let wf = std::fs::read_to_string(o.workdir.join(".github/workflows/ummp-build.yml")).unwrap();
    assert!(wf.contains("java-version: '21'"));
}

#[test]
fn optional_token_is_passed_in_environment_and_redacted_from_debug() {
    let _guard = FAKE_TOOLS_LOCK.lock().unwrap();
    let d = tempfile::tempdir().unwrap();
    let art = d.path().join("art");
    std::fs::create_dir_all(&art).unwrap();
    std::fs::write(
        art.join("stages.json"),
        r#"[{"stage":"build","status":"passed"},{"stage":"test","status":"passed"},{"stage":"server","status":"passed"}]"#,
    )
    .unwrap();
    let (gh, git) = fakes(d.path(), &art, true);
    let mut b = backend(d.path(), gh, git);
    b.token = Some(Secret::new("test-github-token"));
    assert!(!format!("{b:?}").contains("test-github-token"));
    b.run(&request()).unwrap();
    let calls = std::fs::read_to_string(d.path().join("calls")).unwrap();
    assert!(calls.contains("GH_TOKEN=test-github-token"), "{calls}");
    assert!(
        calls
            .lines()
            .filter(|line| line.starts_with("gh "))
            .all(|line| !line.contains("test-github-token")),
        "the token must never appear in command arguments: {calls}"
    );
}

#[test]
fn failed_run_reports_failed_stage() {
    let _guard = FAKE_TOOLS_LOCK.lock().unwrap();
    let d = tempfile::tempdir().unwrap();
    let art = d.path().join("art");
    std::fs::create_dir_all(&art).unwrap();
    std::fs::write(
        art.join("stages.json"),
        r#"[{"stage":"build","status":"failed"},{"stage":"test","status":"skipped"},{"stage":"server","status":"skipped"}]"#,
    )
    .unwrap();
    std::fs::write(
        art.join("build.log"),
        "/home/runner/work/x/x/src/main/java/com/example/demo/DemoWidget.java:5: error: ';' expected\n",
    )
    .unwrap();
    let (gh, git) = fakes(d.path(), &art, true);
    let o = backend(d.path(), gh, git).run(&request()).unwrap();
    assert_eq!(o.stages[0].status, StageStatus::Failed);
    assert_eq!(o.stages[0].errors[0].kind, ErrorKind::Compile);
    assert_eq!(o.stages[1].status, StageStatus::Skipped);
}

#[test]
fn blocked_artifact_download_falls_back_to_job_log() {
    let _guard = FAKE_TOOLS_LOCK.lock().unwrap();
    let d = tempfile::tempdir().unwrap();
    let (gh, git) = fakes(d.path(), &d.path().join("no-artifact"), true);
    let o = backend(d.path(), gh, git).run(&request()).unwrap();
    let statuses: Vec<_> = o.stages.iter().map(|s| s.status).collect();
    assert_eq!(
        statuses,
        vec![
            StageStatus::Passed,
            StageStatus::Passed,
            StageStatus::Failed
        ]
    );
    assert!(
        o.stages[2].log.ends_with("job.log"),
        "{:?}",
        o.stages[2].log
    );
    let e = &o.stages[2].errors[0];
    assert_eq!(e.kind, ummp_core::build::logs::ErrorKind::MissingSymbol);
    assert_eq!(
        e.file.as_deref(),
        Some("src/main/java/com/example/demo/DemoWidget.java")
    );
    assert!(
        o.stages[0].errors.is_empty(),
        "passed stages carry no job-log errors"
    );
}

#[test]
fn gh_missing_suggests_local() {
    let _guard = FAKE_TOOLS_LOCK.lock().unwrap();
    let d = tempfile::tempdir().unwrap();
    let b = backend(
        d.path(),
        "/nonexistent/gh".into(),
        "/nonexistent/git".into(),
    );
    let err = b.run(&request()).unwrap_err().to_string();
    assert!(err.contains("--local"), "{err}");
}

#[test]
fn gh_not_logged_in_suggests_local() {
    let _guard = FAKE_TOOLS_LOCK.lock().unwrap();
    let d = tempfile::tempdir().unwrap();
    let (gh, git) = fakes(d.path(), d.path(), false);
    let err = backend(d.path(), gh, git)
        .run(&request())
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("gh auth login") && err.contains("--local"),
        "{err}"
    );
}
