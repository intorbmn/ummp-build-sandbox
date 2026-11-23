//! Phase 8 local build of `tests/fixtures/build-demo` with the installed Gradle.
//! Skipped (with a message) when Gradle or a JDK is missing.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use ummp_core::build::local::LocalBackend;
use ummp_core::build::logs::ErrorKind;
use ummp_core::build::report::score;
use ummp_core::build::{BuildBackend, BuildRequest, Stage, StageStatus};

fn have(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok()
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/build-demo")
}

fn request(project: &Path) -> BuildRequest {
    BuildRequest {
        project: project.to_path_buf(),
        mod_name: "build-demo".into(),
        stages: vec![Stage::Build, Stage::Test, Stage::Server],
        java: None,
        timeout: Duration::from_mins(5),
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

#[test]
fn local_build_demo_all_pass() {
    if !have("gradle") {
        eprintln!("skipped: no gradle on PATH");
        return;
    }
    let work = tempfile::tempdir().unwrap();
    let backend = LocalBackend {
        workdir: work.path().to_path_buf(),
        gradle: None,
    };
    let o = backend.run(&request(&fixture())).unwrap();
    let statuses: Vec<_> = o.stages.iter().map(|s| s.status).collect();
    assert_eq!(statuses, vec![StageStatus::Passed; 3], "{o:#?}");
    assert_eq!(score(&o), (3, 3));
    assert!(o.workdir.starts_with(work.path()));
    assert!(o
        .workdir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("build-demo-"));
    assert!(o.workdir.join("run/eula.txt").exists());
    assert!(
        !fixture().join("build").exists(),
        "source must stay untouched"
    );
    assert!(
        !fixture().join("run").exists(),
        "source must stay untouched"
    );
}

#[test]
fn compile_error_skips_later_stages() {
    if !have("gradle") {
        eprintln!("skipped: no gradle on PATH");
        return;
    }
    let src = tempfile::tempdir().unwrap();
    copy_dir(&fixture(), src.path());
    let file = src
        .path()
        .join("src/main/java/com/example/demo/DemoWidget.java");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replace("return 4;", "return 4")).unwrap();
    let work = tempfile::tempdir().unwrap();
    let backend = LocalBackend {
        workdir: work.path().to_path_buf(),
        gradle: None,
    };
    let o = backend.run(&request(src.path())).unwrap();
    assert_eq!(o.stages[0].status, StageStatus::Failed);
    assert_eq!(o.stages[1].status, StageStatus::Skipped);
    assert_eq!(o.stages[2].status, StageStatus::Skipped);
    let e = &o.stages[0].errors[0];
    assert_eq!(e.kind, ErrorKind::Compile);
    assert_eq!(
        e.file.as_deref(),
        Some("src/main/java/com/example/demo/DemoWidget.java")
    );
    assert_eq!(score(&o), (0, 1));
}

#[test]
fn missing_gradle_is_reported() {
    let work = tempfile::tempdir().unwrap();
    let backend = LocalBackend {
        workdir: work.path().to_path_buf(),
        gradle: Some(PathBuf::from("/nonexistent/bin/gradle")),
    };
    let o = backend.run(&request(&fixture())).unwrap();
    assert_eq!(o.stages[0].status, StageStatus::Failed);
    assert!(
        o.stages[0].errors[0].message.contains("no Gradle found"),
        "{:?}",
        o.stages[0].errors
    );
    assert_eq!(o.stages[2].status, StageStatus::Skipped);
}
