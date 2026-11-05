use dreamer::{
    chronology::{SourceTime, TimeBasis},
    lesson::{Lesson, LessonStatus},
    lifecycle::{ChangeAction, LessonChange},
    retrieval::RecallQuery,
    store::{LessonStore, WriteOutcome, sqlite::SqliteStore},
};
use serde_json::{Value, json};
const REPO: &str = "example/dreamer-demo";
fn lesson(id: &str, at: &str) -> Lesson {
    let mut value: Value =
        serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap();
    value["id"] = json!(id);
    value["summary"] = json!(format!(
        "Use {} route for account access.",
        if id == "a-old" {
            "legacy"
        } else {
            "replacement"
        }
    ));
    value["applies_when"] =
        json!("For staged account access changes with the same identity-provider context.");
    value["knowledge_key"] = json!("auth.account-access.migration-route");
    value["evidence"] = json!([]);
    value["conversation_evidence"] = json!([{"provider":"codex","thread_id":"00000000-0000-0000-0000-000000000001","turn_id":"turn-1","item_id":id,"source_url":"codex://threads/00000000-0000-0000-0000-000000000001","observed_at":"2026-11-05T11:00:00Z","source_time":{"at":at,"basis":"message_timestamp"},"content_sha256":"a".repeat(64),"kind":"user_statement","claim":"decision","description":"A synthetic user statement about this scoped account access decision."}]);
    serde_json::from_value(value).unwrap()
}
fn insert(store: &mut SqliteStore, l: &Lesson) {
    let token = store.assess_conversation(l).unwrap().assessment_token;
    store.put_assessed_candidate(l, Some(&token)).unwrap();
}
fn change(
    l: &Lesson,
    at: Option<&str>,
    action: ChangeAction,
    target: Option<&str>,
) -> LessonChange {
    let mut proof = l.conversation_evidence.clone();
    proof[0].claim = dreamer::conversation::ConversationClaim::Correction;
    proof[0].source_time = at.map(|at| SourceTime {
        at: at.into(),
        basis: TimeBasis::MessageTimestamp,
    });
    LessonChange {
        repository: REPO.into(),
        source_id: l.id.clone(),
        action,
        target_id: target.map(str::to_owned),
        reason: "A later explicit user correction changes this synthetic scoped account advice."
            .into(),
        producer: "synthetic fixture".into(),
        evidence: vec![],
        conversation_evidence: proof,
    }
}
#[test]
fn source_time_is_not_capture_time_and_v7_basis_is_explicit() {
    let t = SourceTime::from_uuid("017f22e2-79b0-7cc3-98c4-dc0c0c07398f").unwrap();
    assert_eq!(t.datetime().unwrap().timestamp_millis(), 1_645_557_742_000);
    assert_eq!(t.basis, TimeBasis::UuidV7TurnStart);
    assert!(SourceTime::from_uuid("017f22e2-79b0-4cc3-98c4-dc0c0c07398f").is_none());
    let mut l = lesson("a-old", "2026-01-01T00:00:00Z");
    l.conversation_evidence[0].source_time = None;
    assert!(dreamer::chronology::lesson_range(&l).is_none());
    l.conversation_evidence[0].source_time = Some(SourceTime {
        at: "2099-01-01T00:00:00Z".into(),
        basis: TimeBasis::MessageTimestamp,
    });
    assert!(l.validate_candidate().is_err());
}
#[test]
fn ingestion_order_does_not_make_old_advice_newer_and_competition_is_visible() {
    let old = lesson("a-old", "2026-01-01T00:00:00Z");
    let new = lesson("z-new", "2026-02-01T00:00:00Z");
    let mut outcomes = vec![];
    for order in [[&old, &new], [&new, &old]] {
        let d = tempfile::tempdir().unwrap();
        let mut store = SqliteStore::open(&d.path().join("db")).unwrap();
        for l in order {
            insert(&mut store, l);
        }
        let a = store.get(REPO, &old.id).unwrap().unwrap();
        let b = store.get(REPO, &new.id).unwrap().unwrap();
        assert_eq!(a.chronology.as_ref().unwrap().relation, "older_context");
        assert_eq!(b.chronology.as_ref().unwrap().relation, "newer_context");
        assert!(
            a.chronology.unwrap().needs_reconciliation
                && b.chronology.unwrap().needs_reconciliation
        );
        let recalled = store
            .recall(&RecallQuery {
                repository: REPO.into(),
                queries: vec!["account access".into()],
                components: vec![],
                paths: vec![],
                limit: 5,
            })
            .unwrap();
        outcomes.push(
            recalled
                .lessons
                .into_iter()
                .map(|l| l.stored.lesson.id)
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[0], vec![new.id, old.id]);
}
#[test]
fn same_key_requires_current_assessment_and_scope_isolation_is_preserved() {
    let d = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(&d.path().join("db")).unwrap();
    let first = lesson("a-old", "2026-01-01T00:00:00Z");
    insert(&mut store, &first);
    let next = lesson("z-new", "2026-02-01T00:00:00Z");
    assert!(store.put_candidate(&next).is_err());
    assert!(store.get(REPO, &next.id).unwrap().is_none());
    let stale = store.assess_conversation(&next).unwrap().assessment_token;
    let third = lesson("third", "2026-03-01T00:00:00Z");
    insert(&mut store, &third);
    assert!(store.put_assessed_candidate(&next, Some(&stale)).is_err());
    insert(&mut store, &next);
    assert_eq!(
        store.put_assessed_candidate(&next, Some(&stale)).unwrap(),
        WriteOutcome::Unchanged
    );
    let mut other = next.clone();
    other.repository = "example/other".into();
    assert!(
        store
            .assess_conversation(&other)
            .unwrap()
            .related_lessons
            .is_empty()
    );
    store.put_candidate(&other).unwrap();
    let mut unkeyed = next;
    unkeyed.id = "unkeyed".into();
    unkeyed.knowledge_key = None;
    assert!(store.put_candidate(&unkeyed).is_err());
}
#[test]
fn old_equal_unknown_or_proposed_corrections_cannot_deprecate_newer_knowledge() {
    let d = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(&d.path().join("db")).unwrap();
    let old = lesson("a-old", "2026-01-01T00:00:00Z");
    let new = lesson("z-new", "2026-02-01T00:00:00Z");
    insert(&mut store, &old);
    insert(&mut store, &new);
    for at in [
        None,
        Some("2026-01-15T00:00:00Z"),
        Some("2026-02-01T00:00:00Z"),
    ] {
        assert!(
            store
                .change_lesson(&change(&new, at, ChangeAction::Retire, None))
                .is_err()
        );
        // Guard the newer peer even when the caller tries to retire the older lesson.
        assert!(
            store
                .change_lesson(&change(&old, at, ChangeAction::Retire, None))
                .is_err()
        );
    }
    let mut decision = change(
        &new,
        Some("2026-03-01T00:00:00Z"),
        ChangeAction::Retire,
        None,
    );
    decision.conversation_evidence[0].claim = dreamer::conversation::ConversationClaim::Decision;
    assert!(store.change_lesson(&decision).is_err());
    assert_eq!(
        store.get(REPO, &new.id).unwrap().unwrap().lesson.status,
        LessonStatus::Candidate
    );
    let conflict = change(
        &old,
        Some("2026-04-01T00:00:00Z"),
        ChangeAction::Conflict,
        Some(&new.id),
    );
    store.change_lesson(&conflict).unwrap();
    assert!(
        store
            .change_lesson(&change(
                &new,
                Some("2026-03-01T00:00:00Z"),
                ChangeAction::Retire,
                None
            ))
            .is_err()
    );
    let valid = change(
        &new,
        Some("2026-05-01T00:00:00Z"),
        ChangeAction::Retire,
        None,
    );
    store.change_lesson(&valid).unwrap();
    assert_eq!(
        store.change_lesson(&valid).unwrap().0,
        WriteOutcome::Unchanged
    );
    assert_eq!(
        store.get(REPO, &new.id).unwrap().unwrap().lesson.status,
        LessonStatus::Deprecated
    );
}
#[test]
fn schema_six_upgrade_does_not_rewrite_legacy_payload_or_storage_time() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let mut store = SqliteStore::open(&path).unwrap();
    let l = lesson("legacy", "2026-01-01T00:00:00Z");
    insert(&mut store, &l);
    drop(store);
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch(
        "DROP TABLE lesson_knowledge_keys; DELETE FROM schema_migrations WHERE version=7;",
    )
    .unwrap();
    let (raw,time):(String,String)=c.query_row("SELECT payload,(SELECT stored_at FROM lessons WHERE id='legacy') FROM conversation_evidence",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let mut original: Value = serde_json::from_str(&raw).unwrap();
    original.as_object_mut().unwrap().remove("source_time");
    let original = serde_json::to_string(&original).unwrap();
    c.execute("UPDATE conversation_evidence SET payload=?1", [&original])
        .unwrap();
    drop(c);
    let mut store = SqliteStore::open(&path).unwrap();
    let got = store.get(REPO, "legacy").unwrap().unwrap();
    assert_eq!(got.stored_at, time);
    assert!(got.lesson.knowledge_key.is_none());
    assert!(got.lesson.conversation_evidence[0].source_time.is_none());
    assert_eq!(
        store.put_candidate(&got.lesson).unwrap(),
        WriteOutcome::Unchanged
    );
    let c = rusqlite::Connection::open(&path).unwrap();
    let raw: String = c
        .query_row("SELECT payload FROM conversation_evidence", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(raw, original);
}

#[test]
fn approximate_dates_and_mixed_sources_cannot_authorize_terminal_changes() {
    let d = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(&d.path().join("db")).unwrap();
    let l = lesson("approximate", "2026-01-01T00:00:00Z");
    insert(&mut store, &l);
    let mut c = change(&l, Some("2026-03-01T00:00:00Z"), ChangeAction::Retire, None);
    c.conversation_evidence[0]
        .source_time
        .as_mut()
        .unwrap()
        .basis = TimeBasis::TurnTimestamp;
    assert!(store.change_lesson(&c).is_err());
    let mut approximate = l.clone();
    approximate.conversation_evidence[0]
        .source_time
        .as_mut()
        .unwrap()
        .basis = TimeBasis::TurnTimestamp;
    assert_eq!(
        dreamer::chronology::summary(&l, &[approximate], false)
            .unwrap()
            .relation,
        "approximate"
    );
    let mut mixed = lesson("mixed", "2026-01-01T00:00:00Z");
    mixed.knowledge_key = Some("different.context".into());
    let pr: Lesson = serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap();
    mixed.evidence = pr.evidence;
    insert(&mut store, &mixed);
    assert!(
        store
            .change_lesson(&change(
                &mixed,
                Some("2026-03-01T00:00:00Z"),
                ChangeAction::Retire,
                None
            ))
            .is_err()
    );
}

#[test]
fn the_new_correction_can_supersede_old_advice_using_its_own_source() {
    let d = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(&d.path().join("db")).unwrap();
    let old = lesson("a-old", "2026-01-01T00:00:00Z");
    let mut new = lesson("z-new", "2026-02-01T00:00:00Z");
    new.conversation_evidence[0].claim = dreamer::conversation::ConversationClaim::Correction;
    insert(&mut store, &old);
    insert(&mut store, &new);
    let mut c = change(
        &old,
        Some("2026-02-01T00:00:00Z"),
        ChangeAction::Supersede,
        Some(&new.id),
    );
    c.conversation_evidence = new.conversation_evidence.clone();
    assert_eq!(store.assess_change(&c).unwrap()["allowed"], true);
    store.change_lesson(&c).unwrap();
    assert_eq!(
        store.get(REPO, &old.id).unwrap().unwrap().lesson.status,
        LessonStatus::Deprecated
    );
    assert_eq!(
        store.get(REPO, &new.id).unwrap().unwrap().lesson.status,
        LessonStatus::Candidate
    );
}
