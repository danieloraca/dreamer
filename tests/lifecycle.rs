use dreamer::{
    lesson::{Lesson, LessonStatus},
    lifecycle::{ChangeAction, HistoryQuery, LessonChange},
    store::{LessonStore, SearchQuery, WriteOutcome, sqlite::SqliteStore},
};
use rusqlite::Connection;
fn lesson(id: &str) -> Lesson {
    let mut value: Lesson =
        serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap();
    value.id = id.into();
    value
}
fn change(action: ChangeAction, source: &str, target: Option<&str>) -> LessonChange {
    let sample = lesson(source);
    LessonChange {
        repository: sample.repository,
        source_id: source.into(),
        action,
        target_id: target.map(str::to_owned),
        reason: "New inspected PR evidence corrects the earlier advice.".into(),
        producer: "synthetic test agent".into(),
        evidence: sample.evidence,
    }
}
fn history(id: &str) -> HistoryQuery {
    HistoryQuery {
        repository: lesson(id).repository,
        id: id.into(),
        change_id: None,
        limit: 20,
        offset: 0,
    }
}
fn active(store: &SqliteStore, id: &str) -> bool {
    store
        .get(&lesson(id).repository, id)
        .unwrap()
        .unwrap()
        .lesson
        .status
        != LessonStatus::Deprecated
}

#[test]
fn schema_two_upgrade_preserves_knowledge_and_requires_explicit_migration() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("knowledge.sqlite3");
    let original = {
        let mut store = SqliteStore::open(&db).unwrap();
        store.put_candidate(&lesson("original")).unwrap();
        store
            .get(&lesson("original").repository, "original")
            .unwrap()
            .unwrap()
    };
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("DROP TABLE lesson_scope; DELETE FROM schema_migrations WHERE version=4; DROP TABLE lesson_changes; DELETE FROM schema_migrations WHERE version=3;")
        .unwrap();
    drop(connection);
    assert!(SqliteStore::open_read_only(&db).is_err());
    assert!(SqliteStore::open_existing(&db).is_err());
    let store = SqliteStore::open(&db).unwrap();
    assert_eq!(store.schema_version().unwrap(), 4);
    assert_eq!(
        store
            .get(&original.lesson.repository, "original")
            .unwrap()
            .unwrap(),
        original
    );
}
#[test]
fn supersession_preserves_sources_timestamp_and_history_and_is_retry_safe() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("knowledge.sqlite3");
    let mut store = SqliteStore::open(&db).unwrap();
    for id in ["old", "new", "latest"] {
        store.put_candidate(&lesson(id)).unwrap();
    }
    let original = store
        .get(&lesson("old").repository, "old")
        .unwrap()
        .unwrap();
    let proposal = change(ChangeAction::Supersede, "old", Some("new"));
    let (outcome, receipt) = store.change_lesson(&proposal).unwrap();
    assert_eq!(outcome, WriteOutcome::Inserted);
    let retired = store.get(&proposal.repository, "old").unwrap().unwrap();
    assert_eq!(retired.lesson.summary, original.lesson.summary);
    assert_eq!(retired.lesson.evidence, original.lesson.evidence);
    assert_eq!(retired.stored_at, original.stored_at);
    assert_eq!(retired.lesson.status, LessonStatus::Deprecated);
    assert_eq!(
        retired.lifecycle.unwrap().superseded_by.as_deref(),
        Some("new")
    );
    for id in ["old", "new"] {
        assert_eq!(
            store.lesson_history(&history(id)).unwrap().changes,
            vec![receipt.clone()]
        );
    }
    let repeated = store.change_lesson(&proposal).unwrap();
    assert_eq!(repeated.0, WriteOutcome::Unchanged);
    assert_eq!(repeated.1, receipt);
    assert!(
        store
            .change_lesson(&change(ChangeAction::Supersede, "new", Some("old")))
            .is_err()
    );
    store
        .change_lesson(&change(ChangeAction::Supersede, "new", Some("latest")))
        .unwrap();
    assert_eq!(
        store.change_lesson(&proposal).unwrap().0,
        WriteOutcome::Unchanged
    );
    let hits = store
        .search(&SearchQuery {
            repository: proposal.repository,
            text: "pagination".into(),
            limit: 10,
        })
        .unwrap();
    assert_eq!(
        hits.iter()
            .map(|s| s.lesson.id.as_str())
            .collect::<Vec<_>>(),
        vec!["latest"]
    );
    assert!(store.put_candidate(&original.lesson).is_err()); // No automatic resurrection.
    drop(store);
    let reopened = SqliteStore::open_read_only(&db).unwrap();
    assert_eq!(
        reopened.lesson_history(&history("old")).unwrap().changes[0],
        receipt
    );
}
#[test]
fn conflicts_are_symmetric_visible_and_resolve_without_erasing_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(&dir.path().join("db")).unwrap();
    for id in ["a", "b"] {
        store.put_candidate(&lesson(id)).unwrap();
    }
    let proposal = change(ChangeAction::Conflict, "a", Some("b"));
    let receipt = store.change_lesson(&proposal).unwrap().1;
    let reversed = change(ChangeAction::Conflict, "b", Some("a"));
    assert_eq!(
        store.change_lesson(&reversed).unwrap().0,
        WriteOutcome::Unchanged
    );
    for (id, other) in [("a", "b"), ("b", "a")] {
        assert!(active(&store, id));
        assert_eq!(
            store
                .get(&proposal.repository, id)
                .unwrap()
                .unwrap()
                .lifecycle
                .unwrap()
                .conflicts,
            vec![other]
        );
    }
    store
        .change_lesson(&change(ChangeAction::Supersede, "a", Some("b")))
        .unwrap();
    assert!(
        store
            .get(&proposal.repository, "b")
            .unwrap()
            .unwrap()
            .lifecycle
            .is_none()
    );
    let mut page = history("a");
    page.limit = 1;
    let first = store.lesson_history(&page).unwrap();
    assert_eq!(first.changes.len(), 1);
    assert_eq!(first.next_offset, Some(1));
    page.offset = 1;
    let second = store.lesson_history(&page).unwrap();
    assert!(second.next_offset.is_none());
    assert_ne!(first.changes[0].id, second.changes[0].id);
    page.change_id = Some(receipt.id.clone());
    page.offset = 0;
    assert_eq!(store.lesson_history(&page).unwrap().changes, vec![receipt]);
}
#[test]
fn retirement_is_atomic_and_invalid_or_cross_repository_changes_cannot_mutate() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let mut store = SqliteStore::open(&db).unwrap();
    store.put_candidate(&lesson("a")).unwrap();
    let mut elsewhere = lesson("other-only");
    elsewhere.repository = "example/elsewhere".into();
    store.put_candidate(&elsewhere).unwrap();
    for invalid in [
        change(ChangeAction::Supersede, "a", Some("other-only")),
        change(ChangeAction::Conflict, "a", Some("a")),
        change(ChangeAction::Retire, "a", Some("other-only")),
    ] {
        assert!(store.change_lesson(&invalid).is_err());
    }
    let mut unsupported = change(ChangeAction::Retire, "a", None);
    unsupported.evidence.clear();
    assert!(store.change_lesson(&unsupported).is_err());
    let connection = Connection::open(&db).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_projection BEFORE UPDATE OF status ON lessons BEGIN SELECT RAISE(ABORT,'synthetic projection failure'); END;").unwrap();
    let proposal = change(ChangeAction::Retire, "a", None);
    assert!(store.change_lesson(&proposal).is_err());
    assert!(active(&store, "a"));
    assert!(
        store
            .lesson_history(&history("a"))
            .unwrap()
            .changes
            .is_empty()
    );
    connection
        .execute_batch("DROP TRIGGER reject_projection;")
        .unwrap();
    store.change_lesson(&proposal).unwrap();
    assert!(!active(&store, "a"));
    assert!(
        store
            .get(&elsewhere.repository, "other-only")
            .unwrap()
            .unwrap()
            .lifecycle
            .is_none()
    );
    assert!(
        store
            .lesson_history(&HistoryQuery {
                repository: elsewhere.repository,
                ..history("a")
            })
            .unwrap()
            .changes
            .is_empty()
    );
}
