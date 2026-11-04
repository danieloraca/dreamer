use dreamer::{
    lesson::{Lesson, LessonStatus},
    store::{LessonStore, SearchQuery, WriteOutcome, sqlite::SqliteStore},
};
use rusqlite::Connection;
use tempfile::TempDir;

fn fixture() -> Lesson {
    serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap()
}

fn database() -> (TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("knowledge.sqlite3");
    (directory, path)
}

fn query(repository: &str, text: &str, limit: usize) -> SearchQuery {
    SearchQuery {
        repository: repository.into(),
        text: text.into(),
        limit,
    }
}

#[test]
fn migrations_are_repeatable_and_upgrade_an_older_database_without_losing_lessons() {
    let (_directory, path) = database();
    let mut store = SqliteStore::open(&path).unwrap();
    let lesson = fixture();
    store.put_candidate(&lesson).unwrap();
    assert_eq!(store.schema_version().unwrap(), 2);
    drop(store);

    // Reproduce the exact v1 state: core tables/data, no v2 lookup indexes.
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("DROP INDEX lessons_repository_status_time; DROP INDEX evidence_pull_request; DELETE FROM schema_migrations WHERE version = 2;").unwrap();
    drop(connection);
    assert!(SqliteStore::open_read_only(&path).is_err());
    for _ in 0..2 {
        let store = SqliteStore::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 2);
        assert_eq!(
            store
                .get(&lesson.repository, &lesson.id)
                .unwrap()
                .unwrap()
                .lesson,
            lesson
        );
    }
    let connection = Connection::open(&path).unwrap();
    let migrations: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(migrations, 2);
}

#[test]
fn newer_or_gapped_migration_histories_are_rejected_without_changing_data() {
    let (_directory, path) = database();
    drop(SqliteStore::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "INSERT INTO schema_migrations (version, name) VALUES (3, 'future')",
            [],
        )
        .unwrap();
    assert!(SqliteStore::open(&path).is_err());
    assert!(SqliteStore::open_read_only(&path).is_err());
    connection
        .execute("DELETE FROM schema_migrations WHERE version = 1", [])
        .unwrap();
    assert!(SqliteStore::open(&path).is_err());
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn existing_writer_never_creates_or_migrates_a_database() {
    let (_directory, path) = database();
    assert!(SqliteStore::open_existing(&path).is_err());
    assert!(!path.exists());
    drop(SqliteStore::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("DROP INDEX lessons_repository_status_time; DROP INDEX evidence_pull_request; DELETE FROM schema_migrations WHERE version = 2;").unwrap();
    assert!(SqliteStore::open_existing(&path).is_err());
    let version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version, 1);
    drop(connection);
    drop(SqliteStore::open(&path).unwrap());
    assert!(SqliteStore::open_existing(&path).is_ok());
}

#[test]
fn round_trip_survives_reopen_and_preserves_provenance_and_evidence_order() {
    let (_directory, path) = database();
    let mut lesson = fixture();
    let mut second = lesson.evidence[0].clone();
    second.source_url.push_str("-second");
    second.description = "Another synthetic source pointer".into();
    lesson.evidence.push(second);
    let mut store = SqliteStore::open(&path).unwrap();
    assert_eq!(
        store.put_candidate(&lesson).unwrap(),
        WriteOutcome::Inserted
    );
    let original = store.get(&lesson.repository, &lesson.id).unwrap().unwrap();
    assert_eq!(original.lesson, lesson);
    assert!(original.stored_at.ends_with('Z'));
    drop(store);

    let store = SqliteStore::open_read_only(&path).unwrap();
    assert_eq!(
        store.get(&lesson.repository, &lesson.id).unwrap().unwrap(),
        original
    );
    let results = store
        .search(&query(&lesson.repository, "PAGINATION", 5))
        .unwrap();
    assert_eq!(results, vec![original]);
}

#[test]
fn identical_retries_are_noops_and_conflicting_ids_do_not_overwrite() {
    let (_directory, path) = database();
    let mut store = SqliteStore::open(&path).unwrap();
    let mut lesson = fixture();
    store.put_candidate(&lesson).unwrap();
    let original = store.get(&lesson.repository, &lesson.id).unwrap();
    assert_eq!(
        store.put_candidate(&lesson).unwrap(),
        WriteOutcome::Unchanged
    );
    lesson.summary.push_str(" Conflicting revision.");
    assert!(store.put_candidate(&lesson).is_err());
    assert_eq!(store.get(&lesson.repository, &lesson.id).unwrap(), original);
    let connection = Connection::open(&path).unwrap();
    for table in ["lessons", "lesson_evidence", "pull_requests"] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }
}

#[test]
fn conflicting_pr_metadata_rolls_back_every_part_of_the_import() {
    let (_directory, path) = database();
    let mut lesson = fixture();
    let mut conflicting = lesson.evidence[0].clone();
    conflicting.pull_request.title = "Conflicting title for the same revision".into();
    lesson.evidence.push(conflicting);
    let mut store = SqliteStore::open(&path).unwrap();
    assert!(store.put_candidate(&lesson).is_err());
    assert!(store.get(&lesson.repository, &lesson.id).unwrap().is_none());
    let connection = Connection::open(&path).unwrap();
    for table in ["lessons", "lesson_evidence", "pull_requests"] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn imports_require_candidate_status_provenance_and_complete_pr_evidence() {
    let (_directory, path) = database();
    let mut store = SqliteStore::open(&path).unwrap();
    let source = fixture();
    for status in [LessonStatus::Verified, LessonStatus::Deprecated] {
        let mut invalid = source.clone();
        invalid.status = status;
        assert!(store.put_candidate(&invalid).is_err());
    }
    let mut invalid = source.clone();
    invalid.evidence.clear();
    assert!(store.put_candidate(&invalid).is_err());
    invalid = source.clone();
    invalid.provenance.producer.clear();
    assert!(store.put_candidate(&invalid).is_err());
    invalid = source.clone();
    invalid.evidence[0].pull_request.head_sha = "truncated".into();
    assert!(store.put_candidate(&invalid).is_err());
    invalid = source.clone();
    invalid.evidence[0].pull_request.number = 0;
    assert!(store.put_candidate(&invalid).is_err());
    assert!(store.get(&source.repository, &source.id).unwrap().is_none());
}

#[test]
fn search_is_scoped_bounded_literal_and_excludes_deprecated_lessons() {
    let (_directory, path) = database();
    let mut store = SqliteStore::open(&path).unwrap();
    let source = fixture();
    let mut second = source.clone();
    second.id = "second".into();
    let mut other = source.clone();
    other.repository = "example/other".into();
    for lesson in [&source, &second, &other] {
        store.put_candidate(lesson).unwrap();
    }
    assert_eq!(
        store
            .search(&query(&source.repository, "pagination", 100))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        store
            .search(&query(&source.repository, "pagination", 1))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store
            .search(&query(&other.repository, "pagination", 100))
            .unwrap()
            .len(),
        1
    );
    assert!(store.get("example/missing", &source.id).unwrap().is_none());
    assert!(
        store
            .search(&query(&source.repository, "%", 5))
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .search(&query(&source.repository, "' OR 1=1 --", 5))
            .unwrap()
            .is_empty()
    );
    for (text, limit) in [(" ", 5), ("pagination", 0), ("pagination", 101)] {
        assert!(
            store
                .search(&query(&source.repository, text, limit))
                .is_err()
        );
    }
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE lessons SET status = 'deprecated' WHERE repository = ?1 AND id = ?2",
            [&source.repository, &source.id],
        )
        .unwrap();
    let results = store
        .search(&query(&source.repository, "pagination", 5))
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].lesson.id, second.id);
    assert_eq!(
        store
            .get(&source.repository, &source.id)
            .unwrap()
            .unwrap()
            .lesson
            .status,
        LessonStatus::Deprecated
    );
}

#[test]
fn separate_read_connection_sees_committed_writes_and_foreign_keys_hold() {
    let (_directory, path) = database();
    // Also exercise a reopen after the last connection has checkpointed/closed.
    drop(SqliteStore::open(&path).unwrap());
    let mut writer = SqliteStore::open(&path).unwrap();
    let reader = SqliteStore::open_read_only(&path).unwrap();
    let lesson = fixture();
    assert!(
        reader
            .search(&query(&lesson.repository, "pagination", 5))
            .unwrap()
            .is_empty()
    );
    writer.put_candidate(&lesson).unwrap();
    assert_eq!(
        reader
            .search(&query(&lesson.repository, "pagination", 5))
            .unwrap()
            .len(),
        1
    );
    let connection = Connection::open(&path).unwrap();
    let violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    assert!(
        connection
            .execute(
                "UPDATE lesson_evidence SET repository = 'example/unrelated'",
                []
            )
            .is_err()
    );
}
