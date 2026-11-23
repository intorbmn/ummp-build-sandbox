//! Integration tests for the core engine pipeline.

use std::sync::Arc;
use ummp_core::checkpoint::{Checkpoint, CheckpointStore};
use ummp_core::events::{EventBus, EventHandler};
use ummp_core::filesystem::{FileSystem, LocalFileSystem};
use ummp_core::jobs::{Job, JobState, JobStore};
use ummp_core::persistence::JobDb;
use ummp_core::project::{DefaultProjectManager, Project, ProjectManager};
use ummp_core::session::{MigrationSession, SessionConfig, SessionState};
use uuid::Uuid;

fn make_session() -> MigrationSession {
    MigrationSession::new(
        std::path::PathBuf::from("/tmp/project"),
        SessionConfig {
            source_minecraft: "1.20.1".into(),
            target_minecraft: "1.21".into(),
            target_loader: "fabric".into(),
            ai_model: None,
            toolchain_version: "1.0".into(),
        },
    )
}

/// Phase 3 acceptance: import a project, create a session, checkpoint,
/// simulate a failing step that corrupts files, roll back, resume.
#[test]
fn test_full_pipeline_import_session_checkpoint_resume() {
    let dir = tempfile::tempdir().unwrap();
    let snaps = tempfile::tempdir().unwrap();
    let src = dir.path().join("src/main/java/Example.java");
    std::fs::create_dir_all(src.parent().unwrap()).unwrap();
    std::fs::write(&src, "class Example {}").unwrap();

    let pm = DefaultProjectManager;
    let project = pm.open(dir.path()).unwrap();

    let mut session = make_session();
    assert_eq!(session.state, SessionState::Pending);

    session.start("import");
    assert_eq!(session.state, SessionState::Running);
    assert_eq!(session.current_step, Some("import".into()));

    let mut store = CheckpointStore::new();
    let cp = store
        .snapshot(session.id, "import", &project.root, snaps.path())
        .unwrap();
    assert_eq!(store.latest().unwrap().step, "import");

    // A failing transform step corrupts a file and leaves a stray one behind.
    session.start("transform");
    std::fs::write(&src, "class Example { BROKEN").unwrap();
    std::fs::write(dir.path().join("half-written.tmp"), "x").unwrap();
    session.fail();
    assert_eq!(session.state, SessionState::Failed);

    store.restore(cp.id, &project.root).unwrap();
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "class Example {}");
    assert!(!dir.path().join("half-written.tmp").exists());

    // Resume.
    session.start("transform");
    assert_eq!(session.state, SessionState::Running);
    session.succeed();
    assert_eq!(session.state, SessionState::Completed);
}

#[test]
fn test_checkpoint_metadata_roundtrip() {
    let cp = Checkpoint {
        id: Uuid::new_v4(),
        session_id: Uuid::new_v4(),
        step: "import".into(),
        snapshot_path: std::path::PathBuf::from("snap_import"),
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&cp).unwrap();
    let back: Checkpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, cp.id);
    assert_eq!(back.step, "import");
}

#[test]
fn test_job_persistence_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let db = JobDb::open(&dir.path().join("jobs.db")).unwrap();
    let session_id = Uuid::new_v4();

    let mut job = Job::new(session_id, "analyze");
    job.mark_running();
    job.mark_succeeded(serde_json::json!({"ok": true}));
    db.insert(&job).unwrap();

    let jobs = db.by_session(session_id).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].state, JobState::Succeeded);
    assert_eq!(jobs[0].output, Some(serde_json::json!({"ok": true})));
}

#[test]
fn test_event_bus_pipeline_events() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Recorder(Arc<AtomicUsize>);
    impl EventHandler for Recorder {
        fn handle(&self, _event: &ummp_core::events::Event) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let bus = EventBus::new();
    let count = Arc::new(AtomicUsize::new(0));
    bus.subscribe(Arc::new(Recorder(count.clone())));

    bus.publish("step.start", serde_json::json!({"step": "import"}));
    bus.publish("step.done", serde_json::json!({"step": "import"}));

    assert_eq!(count.load(Ordering::SeqCst), 2);
    let events = bus.drain();
    assert_eq!(events.len(), 2);
}

#[test]
fn test_metadata_roundtrip_via_filesystem() {
    let dir = tempfile::tempdir().unwrap();
    let fs = LocalFileSystem;

    let project = Project::new(dir.path().to_path_buf());
    let meta_path = project.root.join("ummp.json");
    let json = r#"{"mods":[{"mod_id":"test","version":"1.0.0","name":"Test Mod","loader":"Fabric","minecraft":{"major":1,"minor":20,"patch":1},"dependencies":[]}],"languages":["java"],"build_system":{"kind":"Gradle","file":"build.gradle"}}"#;
    fs.write(&meta_path, json.as_bytes()).unwrap();

    let pm = DefaultProjectManager;
    let meta = pm.metadata(&project).unwrap();
    assert_eq!(meta.mods.len(), 1);
    assert_eq!(meta.mods[0].mod_id, "test");
    assert_eq!(meta.languages, vec!["java"]);
    assert!(meta.build_system.is_some());
}

#[test]
fn test_session_job_store_consistency() {
    let session_id = Uuid::new_v4();
    let mut store = JobStore::new();
    store.add(Job::new(session_id, "a"));
    store.add(Job::new(session_id, "b"));
    store.add(Job::new(Uuid::new_v4(), "c"));

    let jobs = store.for_session(session_id);
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().all(|j| j.session_id == session_id));
}
