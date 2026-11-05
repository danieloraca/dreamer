use dreamer::{
    activity::{LearningRun, RunPhase, RunQuery, RunState},
    lesson::Lesson,
    store::{LessonStore, WriteOutcome, sqlite::SqliteStore},
};
fn run(n: usize) -> LearningRun {
    LearningRun {
        repository: "example/project".into(),
        id: format!("{n:064x}"),
        phase: RunPhase::Publication,
        state: RunState::Started,
        client_started_at: "2026-11-05T12:00:00Z".into(),
        client_finished_at: None,
        conversation: None,
        pr: Some(12),
        revision: Some("a".repeat(64)),
        inserted: None,
        reused: None,
        changed: None,
    }
}
fn query(repo: &str) -> RunQuery {
    RunQuery {
        repository: repo.into(),
        limit: 10,
        offset: 0,
    }
}
#[test]
fn reports_are_scoped_bounded_idempotent_and_terminal_outcomes_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("knowledge.sqlite3");
    let mut store = SqliteStore::open(&path).unwrap();
    let started = run(1);
    assert_eq!(store.record_run(&started).unwrap(), WriteOutcome::Inserted);
    assert_eq!(store.record_run(&started).unwrap(), WriteOutcome::Unchanged);
    let mut finished = started.clone();
    finished.state = RunState::Succeeded;
    finished.client_finished_at = Some("2026-11-05T12:01:00Z".into());
    finished.inserted = Some(2);
    finished.reused = Some(1);
    finished.changed = Some(0);
    store.record_run(&finished).unwrap();
    assert_eq!(store.record_run(&started).unwrap(), WriteOutcome::Unchanged);
    let mut changed = finished.clone();
    changed.inserted = Some(3);
    assert!(store.record_run(&changed).is_err());
    let summary = store.runs(&query("example/project")).unwrap().summary;
    assert_eq!(
        (summary.retained, summary.incomplete, summary.failures),
        (1, 0, 0)
    );
    assert!(summary.last_learning_report_at.is_some());
    let mut failed = run(2);
    failed.state = RunState::Failed;
    failed.client_finished_at = Some("2026-11-05T12:02:00Z".into());
    store.record_run(&failed).unwrap();
    assert_eq!(
        store
            .runs(&query("example/project"))
            .unwrap()
            .summary
            .failures,
        1
    );
    let mut other = run(1);
    other.repository = "example/other".into();
    store.record_run(&other).unwrap();
    for n in 3..505 {
        store.record_run(&run(n)).unwrap();
    }
    assert_eq!(
        store
            .runs(&query("example/project"))
            .unwrap()
            .summary
            .retained,
        500
    );
    assert_eq!(
        store
            .runs(&query("example/other"))
            .unwrap()
            .summary
            .retained,
        1
    );
    let mut q = query("example/project");
    q.limit = 1;
    let page = store.runs(&q).unwrap();
    assert_eq!(page.next_offset, Some(1));
    q.offset = 1;
    assert_ne!(store.runs(&q).unwrap().runs[0].run.id, page.runs[0].run.id);
    assert!(serde_json::from_value::<LearningRun>(serde_json::json!({"error":"secret"})).is_err());
    failed.inserted = Some(0);
    assert!(store.record_run(&failed).is_err());
    q.limit = 51;
    assert!(store.runs(&q).is_err());
}
#[test]
fn schema_four_upgrades_without_changing_knowledge_and_checker_never_migrates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("knowledge.sqlite3");
    let mut store = SqliteStore::open(&path).unwrap();
    let lesson: Lesson =
        serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap();
    store.put_candidate(&lesson).unwrap();
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE lesson_knowledge_keys; DELETE FROM schema_migrations WHERE version=7; DROP TABLE conversation_evidence; DELETE FROM schema_migrations WHERE version=6; DROP TABLE learning_runs; DELETE FROM schema_migrations WHERE version=5;")
        .unwrap();
    drop(connection);
    assert!(SqliteStore::check_file(&path, false).is_err());
    assert_eq!(SqliteStore::check_file(&path, true).unwrap(), 4);
    assert!(SqliteStore::open_existing(&path).is_err());
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(
        store
            .get(&lesson.repository, &lesson.id)
            .unwrap()
            .unwrap()
            .lesson,
        lesson
    );
    assert_eq!(SqliteStore::check_file(&path, false).unwrap(), 7);
    let missing = dir.path().join("missing.sqlite3");
    assert!(SqliteStore::check_file(&missing, true).is_err());
    assert!(!missing.exists());
}
#[test]
fn checked_snapshot_restores_a_readable_store_without_replacing_live_knowledge() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    std::fs::create_dir(&state).unwrap();
    let path = state.join("knowledge.sqlite3");
    let mut store = SqliteStore::open(&path).unwrap();
    let lesson: Lesson =
        serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap();
    store.put_candidate(&lesson).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/backup.py");
    let config = dir.path().join("backup.json");
    std::fs::write(
        &config,
        serde_json::to_vec(&serde_json::json!({"directory":dir.path().join("backups")})).unwrap(),
    )
    .unwrap();
    let backup = Command::new("python3")
        .arg(&script)
        .args(["create", "--database"])
        .arg(&path)
        .arg("--config")
        .arg(&config)
        .output()
        .unwrap();
    assert!(backup.status.success(), "{:?}", backup);
    let manifest: serde_json::Value = serde_json::from_slice(&backup.stdout).unwrap();
    let snapshot = dir
        .path()
        .join("backups")
        .join(manifest["filename"].as_str().unwrap());
    let restored = dir.path().join("restored/knowledge.sqlite3");
    let output = Command::new("python3")
        .arg(script)
        .args(["restore", "--snapshot"])
        .arg(snapshot)
        .arg("--output")
        .arg(&restored)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(SqliteStore::check_file(&restored, false).unwrap(), 7);
    assert_eq!(
        SqliteStore::open_read_only(&restored)
            .unwrap()
            .get(&lesson.repository, &lesson.id)
            .unwrap()
            .unwrap()
            .lesson,
        lesson
    );
    assert_eq!(
        store
            .get(&lesson.repository, &lesson.id)
            .unwrap()
            .unwrap()
            .lesson,
        lesson
    );
    let output = Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .args(["--config", "/does/not/exist", "db", "check", "--file"])
        .arg(restored)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
}
