use dreamer::{
    lesson::{Lesson, LessonStatus},
    retrieval::{LessonScope, RecallQuery},
    store::{LessonStore, SearchQuery, WriteOutcome, sqlite::SqliteStore},
};
use rusqlite::Connection;
use serde_json::{Value, json};
fn fixture() -> Lesson {
    serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap()
}
fn query(terms: &[&str]) -> RecallQuery {
    RecallQuery {
        repository: fixture().repository,
        queries: terms.iter().map(|s| (*s).into()).collect(),
        components: vec![],
        paths: vec![],
        limit: 5,
    }
}
fn seed(store: &mut SqliteStore) -> Value {
    let cohort: Value =
        serde_json::from_str(include_str!("../fixtures/retrieval-cases.json")).unwrap();
    for row in cohort["lessons"].as_array().unwrap() {
        let mut value = serde_json::to_value(fixture()).unwrap();
        for (key, item) in row.as_object().unwrap() {
            value[key] = item.clone();
        }
        store
            .put_candidate(&serde_json::from_value(value).unwrap())
            .unwrap();
    }
    cohort
}
#[test]
fn fixed_cohort_improves_relevance_without_treating_scores_as_correctness() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let mut store = SqliteStore::open(&db).unwrap();
    let cohort = seed(&mut store);
    // A fixed timestamp makes the old lexical tie order reproducible.
    Connection::open(&db)
        .unwrap()
        .execute("UPDATE lessons SET stored_at='2026-01-01T00:00:00Z'", [])
        .unwrap();
    let mut results = Vec::new();
    let mut legacy_success = 0;
    let mut ranked_success = 0;
    for case in cohort["cases"].as_array().unwrap() {
        let q:RecallQuery=serde_json::from_value(json!({"repository":fixture().repository,"queries":case["queries"],"paths":case.get("paths").cloned().unwrap_or(json!([])),"components":case.get("components").cloned().unwrap_or(json!([])),"limit":5})).unwrap();
        let expected: Vec<String> = serde_json::from_value(case["expected"].clone()).unwrap();
        let mut old = std::collections::BTreeSet::new();
        for term in &q.queries {
            for stored in store
                .search(&SearchQuery {
                    repository: q.repository.clone(),
                    text: term.clone(),
                    limit: 20,
                })
                .unwrap()
            {
                old.insert(stored.lesson.id);
            }
        }
        let old: Vec<_> = old.into_iter().collect();
        let page = store.recall(&q).unwrap();
        let current: Vec<_> = page
            .lessons
            .iter()
            .map(|hit| hit.stored.lesson.id.clone())
            .collect();
        assert_eq!(current, expected, "{}", case["name"]);
        for excluded in case["absent"].as_array().into_iter().flatten() {
            assert!(!current.iter().any(|id| id == excluded.as_str().unwrap()));
        }
        let old_top = old.first();
        let new_top = current.first();
        legacy_success += usize::from(old_top == expected.first());
        ranked_success += usize::from(new_top == expected.first());
        results.push(
            json!({"case":case["name"],"legacy_ids":old,"ranked_ids":current,"expected":expected}),
        );
    }
    assert!(ranked_success > legacy_success);
    println!(
        "RETRIEVAL_EVALUATION={}",
        json!({"synthetic_cases":results.len(),"legacy_top_result_or_correct_empty":legacy_success,"ranked_top_result_or_correct_empty":ranked_success,"cases":results,"real_review_uplift_claimed":false})
    );
}
#[test]
fn scope_round_trips_and_conflicts_without_rewriting_legacy_records() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let mut store = SqliteStore::open(&db).unwrap();
    let old = fixture();
    store.put_candidate(&old).unwrap();
    let mut scoped = old.clone();
    scoped.id = "scoped".into();
    scoped.scope = LessonScope {
        components: vec!["exports".into()],
        paths: vec!["src/Export/".into()],
        excluded_terms: vec!["public exports".into()],
    };
    assert_eq!(
        store.put_candidate(&scoped).unwrap(),
        WriteOutcome::Inserted
    );
    assert_eq!(
        store.put_candidate(&scoped).unwrap(),
        WriteOutcome::Unchanged
    );
    let mut conflict = scoped.clone();
    conflict.scope.paths.push("other/".into());
    assert!(store.put_candidate(&conflict).is_err());
    drop(store);
    let store = SqliteStore::open_read_only(&db).unwrap();
    assert_eq!(
        store
            .get(&scoped.repository, &scoped.id)
            .unwrap()
            .unwrap()
            .lesson,
        scoped
    );
    assert_eq!(
        store.get(&old.repository, &old.id).unwrap().unwrap().lesson,
        old
    );
}
#[test]
fn schema_three_upgrade_preserves_history_and_normalized_candidate_data() {
    use dreamer::lifecycle::{ChangeAction, HistoryQuery, LessonChange};
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let mut store = SqliteStore::open(&db).unwrap();
    let old = fixture();
    store.put_candidate(&old).unwrap();
    let change = LessonChange {
        repository: old.repository.clone(),
        source_id: old.id.clone(),
        action: ChangeAction::Retire,
        target_id: None,
        reason: "Synthetic reviewed evidence retires the earlier claim.".into(),
        producer: "fixture".into(),
        evidence: old.evidence,
    };
    let receipt = store.change_lesson(&change).unwrap().1;
    drop(store);
    Connection::open(&db)
        .unwrap()
        .execute_batch("DROP TABLE lesson_scope; DELETE FROM schema_migrations WHERE version=4;")
        .unwrap();
    assert!(SqliteStore::open_existing(&db).is_err());
    let store = SqliteStore::open(&db).unwrap();
    assert_eq!(store.schema_version().unwrap(), 4);
    let item = store.get(&old.repository, &old.id).unwrap().unwrap();
    assert_eq!(item.lesson.status, LessonStatus::Deprecated);
    assert!(item.lesson.scope.is_empty());
    assert_eq!(
        store
            .lesson_history(&HistoryQuery {
                repository: old.repository,
                id: old.id,
                change_id: None,
                limit: 20,
                offset: 0
            })
            .unwrap()
            .changes,
        vec![receipt]
    );
}
#[test]
fn ranked_recall_keeps_scope_literal_queries_path_boundaries_and_negated_safety_rules() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(&dir.path().join("db")).unwrap();
    seed(&mut store);
    let mut elsewhere = fixture();
    elsewhere.id = "b-exports".into();
    elsewhere.repository = "example/other".into();
    elsewhere.summary = "export export export export".into();
    store.put_candidate(&elsewhere).unwrap();
    assert!(
        store
            .recall(&query(&["' OR 1=1 --"]))
            .unwrap()
            .lessons
            .is_empty()
    );
    let mut q = query(&["opaque-task"]);
    q.paths = vec!["src/JobsElsewhere/Foo.rs".into()];
    assert!(store.recall(&q).unwrap().lessons.is_empty());
    let mut broad = fixture();
    broad.id = "a-broad-path".into();
    broad.summary = "A generic source tree convention.".into();
    broad.scope.paths = vec!["src/".into()];
    store.put_candidate(&broad).unwrap();
    q.paths = vec!["src/Jobs/Delivery.rs".into()];
    assert_eq!(
        store.recall(&q).unwrap().lessons[0].stored.lesson.id,
        "e-queues"
    );
    q.paths = vec!["../secret".into()];
    assert!(store.recall(&q).is_err());
    let mut safety = fixture();
    safety.id = "safety".into();
    safety.summary = "Do not skip authorization when copying records.".into();
    safety.applies_when = "Not only exports: check every copy boundary.".into();
    store.put_candidate(&safety).unwrap();
    assert!(
        store
            .recall(&query(&["authorization"]))
            .unwrap()
            .lessons
            .iter()
            .any(|hit| hit.stored.lesson.id == "safety")
    );
    assert!(
        store
            .recall(&query(&["export"]))
            .unwrap()
            .lessons
            .iter()
            .all(|hit| hit.stored.lesson.repository == fixture().repository)
    );
}
