use dreamer::{
    conversation::{ConversationClaim, ConversationEvidence, ConversationKind},
    lesson::Lesson,
    lifecycle::{ChangeAction, HistoryQuery, LessonChange},
    retrieval::RecallQuery,
    store::{LessonStore, WriteOutcome, sqlite::SqliteStore},
};
#[cfg(feature = "mac-client")]
use serde_json::{Value, json};
const REPO: &str = "example/dreamer-demo";
const THREAD: &str = "00000000-0000-0000-0000-000000000001";
fn pr() -> Lesson {
    serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap()
}
fn chat() -> Lesson {
    let mut l = pr();
    l.id = "chat-fixture".into();
    l.summary = "Keep existing account access until replacement validation completes.".into();
    l.applies_when =
        "For staged access changes; configuration readback is separate from customer acceptance."
            .into();
    l.evidence.clear();
    l.knowledge_key = Some("fixture.staged-access".into());
    l.conversation_evidence = vec![ConversationEvidence {
        source_time: Some(dreamer::chronology::SourceTime {
            at: "2026-01-01T00:00:00Z".into(),
            basis: dreamer::chronology::TimeBasis::MessageTimestamp,
        }),
        provider: "codex".into(),
        thread_id: THREAD.into(),
        turn_id: "turn-1".into(),
        item_id: "user-1".into(),
        source_url: format!("codex://threads/{THREAD}"),
        observed_at: "2026-11-05T11:00:00Z".into(),
        content_sha256: "a".repeat(64),
        kind: ConversationKind::UserStatement,
        claim: ConversationClaim::Decision,
        description:
            "Synthetic inspected user decision; technical end-to-end validation remains pending."
                .into(),
    }];
    l
}
#[test]
fn schema_five_upgrade_preserves_pr_json_and_accepts_mixed_sources() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = SqliteStore::open(&path).unwrap();
    let old = pr();
    store.put_candidate(&old).unwrap();
    let before = store.get(REPO, &old.id).unwrap().unwrap();
    drop(store);
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch(
        "DROP TABLE lesson_knowledge_keys; DELETE FROM schema_migrations WHERE version=7; DROP TABLE conversation_evidence; DELETE FROM schema_migrations WHERE version=6;",
    )
    .unwrap();
    drop(c);
    assert_eq!(SqliteStore::check_file(&path, true).unwrap(), 5);
    assert!(SqliteStore::open_read_only(&path).is_err());
    let mut store = SqliteStore::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), 7);
    assert_eq!(store.get(REPO, &old.id).unwrap().unwrap(), before);
    assert!(
        serde_json::to_value(&before.lesson)
            .unwrap()
            .get("conversation_evidence")
            .is_none()
    );
    let mut mixed = chat();
    mixed.evidence = old.evidence;
    store.put_candidate(&mixed).unwrap();
    assert_eq!(store.get(REPO, &mixed.id).unwrap().unwrap().lesson, mixed);
    let counts = store
        .browse(&dreamer::store::BrowseQuery {
            repository: REPO.into(),
            text: "".into(),
            status: None,
            offset: 0,
            limit: 50,
        })
        .unwrap()
        .counts;
    assert_eq!(
        (counts.total, counts.source_prs, counts.source_chats),
        (2, 1, 1)
    );
}
#[test]
fn chat_recall_and_correction_history_remain_scoped_and_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(&dir.path().join("db")).unwrap();
    let l = chat();
    assert_eq!(store.put_candidate(&l).unwrap(), WriteOutcome::Inserted);
    assert_eq!(store.put_candidate(&l).unwrap(), WriteOutcome::Unchanged);
    let mut other = l.clone();
    other.repository = "example/other".into();
    store.put_candidate(&other).unwrap();
    let query = RecallQuery {
        repository: REPO.into(),
        queries: vec!["account access".into()],
        components: vec![],
        paths: vec![],
        limit: 3,
    };
    assert_eq!(store.recall(&query).unwrap().lessons.len(), 1);
    let change = LessonChange {
        repository: REPO.into(),
        source_id: l.id.clone(),
        action: ChangeAction::Retire,
        target_id: None,
        reason: "Synthetic later user decision withdraws the earlier access advice.".into(),
        producer: "fixture".into(),
        evidence: vec![],
        conversation_evidence: l.conversation_evidence.clone(),
    };
    let mut change = change;
    change.conversation_evidence[0].claim = ConversationClaim::Correction;
    change.conversation_evidence[0]
        .source_time
        .as_mut()
        .unwrap()
        .at = "2026-01-02T00:00:00Z".into();
    store.change_lesson(&change).unwrap();
    assert!(store.recall(&query).unwrap().lessons.is_empty());
    assert_eq!(
        store
            .lesson_history(&HistoryQuery {
                repository: REPO.into(),
                id: l.id,
                change_id: None,
                offset: 0,
                limit: 10
            })
            .unwrap()
            .changes[0]
            .change,
        change
    );
    assert_eq!(
        store
            .get("example/other", &other.id)
            .unwrap()
            .unwrap()
            .lesson,
        other
    );
}
#[test]
fn assistant_claims_cannot_masquerade_as_observed_results() {
    let mut l = chat();
    l.conversation_evidence[0].kind = ConversationKind::AssistantReport;
    assert!(l.validate_candidate().is_err());
    l.conversation_evidence[0].claim = ConversationClaim::ObservedOutcome;
    assert!(l.validate_candidate().is_err());
    l.conversation_evidence[0].claim = ConversationClaim::Hypothesis;
    assert!(l.validate_candidate().is_err());
    l.summary = format!("Hypothesis (unverified): {}", l.summary);
    assert!(l.validate_candidate().is_ok());
    l.conversation_evidence[0].source_url = "javascript:alert(1)".into();
    assert!(l.validate_candidate().is_err());
}

#[cfg(feature = "mac-client")]
mod mac {
    use super::*;
    use dreamer::{
        config::Config,
        mac::{
            Settings,
            conversations::{
                self, ChatChangeDraft, ChatDraft, ChatReadInput, Observation, PrepareChatInput,
                ReconcileChatInput, RememberChatInput,
            },
        },
        mcp::http::{self, Credentials},
    };
    use std::{fs, path::Path};
    use tokio_util::sync::CancellationToken;
    const WRITE: &str = "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww";
    fn settings(root: &Path) -> Settings {
        let token = root.join("writer.env");
        dreamer::collector::write_private(
            &token,
            format!("DREAMER_MCP_WRITE_TOKEN={WRITE}\n").as_bytes(),
        )
        .unwrap();
        Settings {
            repository: REPO.into(),
            repositories: vec![REPO.into(), "example/other".into()],
            endpoint: "http://127.0.0.1:1/mcp".into(),
            writer_token_file: token,
            cache_dir: root.join("cache"),
            learn_conversations: true,
        }
    }
    fn input() -> PrepareChatInput {
        PrepareChatInput{repository:REPO.into(),thread_id:THREAD.into(),page:Some(serde_json::from_str(include_str!("../fixtures/codex-conversation.json")).unwrap()),observations:vec![Observation{item_id:"tool-1".into(),text:"The inspected configuration readback matches; customer acceptance has not been tested.".into()}],sensitive_terms:vec!["Customer Alpha".into()],fragment_offset:None}
    }
    fn read(s: &Settings, v: &Value) {
        conversations::read(
            s,
            ChatReadInput {
                repository: REPO.into(),
                revision: v["revision"].as_str().unwrap().into(),
                ids: vec!["user-1".into(), "assistant-1".into(), "tool-1".into()],
            },
        )
        .unwrap();
    }
    fn draft() -> ChatDraft {
        ChatDraft {
            knowledge_key: Some("fixture.staged-access".into()),
            assessment_token: None,
            related_assessments: vec![],
            summary: chat().summary,
            applies_when: chat().applies_when,
            claim: ConversationClaim::Decision,
            scope: Default::default(),
            evidence_ids: vec!["user-1".into()],
        }
    }
    fn remember(v: &Value, drafts: Vec<ChatDraft>) -> RememberChatInput {
        RememberChatInput {
            repository: REPO.into(),
            revision: v["revision"].as_str().unwrap().into(),
            lessons: drafts,
            skip_reason: None,
        }
    }
    #[test]
    fn filters_before_retention_and_rejects_unread_or_unknown_scope() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = settings(dir.path());
        s.learn_conversations = false;
        assert!(conversations::prepare(&s, input()).is_err());
        assert!(!s.cache_dir.exists());
        s.learn_conversations = true;
        let v = conversations::prepare(&s, input()).unwrap();
        let rev = v["revision"].as_str().unwrap();
        assert_eq!(v["evidence"].as_array().unwrap().len(), 3);
        assert_eq!(v["has_more"], true);
        let raw =
            fs::read_to_string(s.cache_dir.join("chats").join(rev).join("snapshot.json")).unwrap();
        for forbidden in [
            "SECRET_ARGUMENT",
            "PRIVATE_REASONING",
            "RAW_COMMAND",
            "RAW_OUTPUT",
            "person@example.com",
            "Customer Alpha",
            "UNFINISHED_TURN",
            "TRANSIENT_COMMENTARY",
        ] {
            assert!(!raw.contains(forbidden), "{forbidden}");
        }
        assert!(conversations::build(&s, REPO, rev, &[draft()]).is_err());
        read(&s, &v);
        let l = conversations::build(&s, REPO, rev, &[draft()])
            .unwrap()
            .remove(0);
        assert!(l.evidence.is_empty());
        assert_eq!(l.conversation_evidence[0].thread_id, THREAD);
        assert!(
            conversations::read(
                &s,
                ChatReadInput {
                    repository: "example/other".into(),
                    revision: rev.into(),
                    ids: vec!["user-1".into()]
                }
            )
            .is_err()
        );
        let mut bad = input();
        bad.thread_id = "00000000-0000-0000-0000-000000000002".into();
        assert!(conversations::prepare(&s, bad).is_err());
    }
    #[test]
    fn revised_turn_invalidates_old_sources_and_assistant_reports_are_not_results() {
        let dir = tempfile::tempdir().unwrap();
        let s = settings(dir.path());
        let v = conversations::prepare(&s, input()).unwrap();
        read(&s, &v);
        let mut d = draft();
        d.claim = ConversationClaim::ObservedOutcome;
        d.evidence_ids = vec!["assistant-1".into()];
        assert!(
            conversations::build(&s, REPO, v["revision"].as_str().unwrap(), &[d.clone()]).is_err()
        );
        d.evidence_ids.push("tool-1".into());
        assert!(conversations::build(&s, REPO, v["revision"].as_str().unwrap(), &[d]).is_ok());
        let mut newer = input();
        newer.page.as_mut().unwrap()["turns"][0]["items"][0]["content"][0]["text"] = json!(
            "Keep access unchanged until independent acceptance; the earlier decision is revised."
        );
        let next = conversations::prepare(&s, newer).unwrap();
        assert_ne!(next["revision"], v["revision"]);
        assert_eq!(next["changed_item_ids"], json!(["user-1"]));
        assert!(
            conversations::build(&s, REPO, v["revision"].as_str().unwrap(), &[draft()]).is_err()
        );
    }
    #[test]
    fn result_status_or_truncated_output_cannot_prove_an_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let s = settings(dir.path());
        let mut missing = input();
        missing.page.as_mut().unwrap()["turns"][0]["items"][3]
            .as_object_mut()
            .unwrap()
            .remove("result");
        assert!(conversations::prepare(&s, missing).is_err());
        let mut truncated = input();
        truncated.page.as_mut().unwrap()["turns"][0]["items"][3]["outputTruncated"] = json!(true);
        assert!(conversations::prepare(&s, truncated).is_err());
        let mut failed = input();
        failed.page.as_mut().unwrap()["turns"][0]["items"][3]["status"] = json!("failed");
        assert!(
            conversations::prepare(&s, failed).is_ok(),
            "observed failures are useful, not successful fixes"
        );
    }
    async fn backend(
        s: &mut Settings,
        root: &Path,
    ) -> (
        CancellationToken,
        tokio::task::JoinHandle<()>,
        std::path::PathBuf,
    ) {
        let pi = root.join("pi");
        fs::create_dir(&pi).unwrap();
        let db = pi.join("knowledge.sqlite3");
        drop(SqliteStore::open(&db).unwrap());
        let config=Config::parse(&format!("data_dir={pi:?}\n[github]\nrepository=\"{REPO}\"\nbatch_size=10\n[memory]\ndefault_search_limit=5\n[http]\nlisten=\"127.0.0.1:0\"\nallowed_hosts=[\"127.0.0.1\"]")).unwrap();
        let stop = CancellationToken::new();
        let router = http::router(
            &config,
            Credentials::new("r".repeat(64), Some(WRITE.into())).unwrap(),
            stop.clone(),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        s.endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let stopping = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stopping.cancelled_owned())
                .await
                .unwrap();
        });
        (stop, task, db)
    }
    #[tokio::test]
    async fn fragment_bundles_publish_original_identity_and_retry_without_duplicates() {
        use conversations::{ChatBundleInput, bundle, stage};
        let dir = tempfile::tempdir().unwrap();
        let mut s = settings(dir.path());
        let (stop, task, db) = backend(&mut s, dir.path()).await;
        let mut source = input();
        source.observations.clear();
        source.fragment_offset = Some(0);
        source.page.as_mut().unwrap()["turns"][0]["items"] = json!([{"id":"original-message","type":"agentMessage","phase":"final_answer","text":format!("{} Last corrective detail.","Synthetic engineering context. ".repeat(180))}]);
        let captured = conversations::prepare(&s, source).unwrap();
        let revision = captured["revision"].as_str().unwrap();
        let ids: Vec<String> = captured["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap().into())
            .collect();
        conversations::read(
            &s,
            ChatReadInput {
                repository: REPO.into(),
                revision: revision.into(),
                ids: ids.clone(),
            },
        )
        .unwrap();
        stage(
            &s,
            RememberChatInput {
                repository: REPO.into(),
                revision: revision.into(),
                lessons: vec![],
                skip_reason: Some(
                    "Consolidate this context before deriving a complete supported claim.".into(),
                ),
            },
        )
        .unwrap();
        let input = ChatBundleInput {
            repository: REPO.into(),
            thread_id: THREAD.into(),
            revisions: vec![revision.into()],
            evidence_ids: vec![ids[0].clone(), ids[1].clone()],
            resolutions: vec![],
        };
        let bundled = bundle(&s, input).unwrap();
        let revision = bundled["revision"].as_str().unwrap();
        let mut d = draft();
        d.claim = ConversationClaim::Hypothesis;
        d.summary="Hypothesis (unverified): Preserve original identity when combining inspected source fragments.".into();
        d.evidence_ids = vec![ids[0].clone(), ids[1].clone()];
        // Delivered identical sources inherit inspection; the bundle need not resend the text.
        let expected = conversations::build(&s, REPO, revision, &[d.clone()])
            .unwrap()
            .remove(0);
        assert_eq!(expected.conversation_evidence.len(), 1);
        assert_eq!(
            expected.conversation_evidence[0].item_id,
            "original-message"
        );
        let request = RememberChatInput {
            repository: REPO.into(),
            revision: revision.into(),
            lessons: vec![d.clone()],
            skip_reason: None,
        };
        let first = conversations::remember(&s, request).await.unwrap();
        assert_eq!(first["outcomes"][0]["outcome"], "inserted");
        let retry = conversations::remember(
            &s,
            RememberChatInput {
                repository: REPO.into(),
                revision: revision.into(),
                lessons: vec![d],
                skip_reason: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(retry["outcomes"][0]["outcome"], "already_known");
        assert_eq!(
            SqliteStore::open_read_only(&db)
                .unwrap()
                .get(REPO, &expected.id)
                .unwrap()
                .unwrap()
                .lesson,
            expected
        );
        stop.cancel();
        task.await.unwrap();
    }
    #[tokio::test]
    async fn actual_http_path_publishes_deduplicates_and_corrects_chat_knowledge() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = settings(dir.path());
        let (stop, task, db) = backend(&mut s, dir.path()).await;
        let v = conversations::prepare(&s, input()).unwrap();
        read(&s, &v);
        let l = conversations::build(&s, REPO, v["revision"].as_str().unwrap(), &[draft()])
            .unwrap()
            .remove(0);
        let result = conversations::remember(&s, remember(&v, vec![draft()]))
            .await
            .unwrap();
        assert_eq!(result["complete"], true);
        assert_eq!(result["outcomes"][0]["outcome"], "inserted");
        assert_eq!(
            SqliteStore::open_read_only(&db)
                .unwrap()
                .get(REPO, &l.id)
                .unwrap()
                .unwrap()
                .lesson,
            l
        );
        assert_eq!(
            conversations::remember(&s, remember(&v, vec![draft()]))
                .await
                .unwrap()["outcomes"][0]["outcome"],
            "already_known"
        );
        assert_eq!(
            conversations::prepare(&s, input()).unwrap()["already_processed"],
            true
        );
        let mut newer = input();
        newer.page.as_mut().unwrap()["turns"][0]["startedAt"] = json!("2026-01-02T00:00:00Z");
        newer.page.as_mut().unwrap()["turns"][0]["items"][0]["createdAt"] =
            json!("2026-01-02T00:00:00Z");
        let corrected = conversations::prepare(&s, newer).unwrap();
        read(&s, &corrected);
        let correction = ChatChangeDraft {
            source_id: l.id.clone(),
            action: ChangeAction::Retire,
            target_id: None,
            reason:
                "A later explicit user correction withdraws the earlier synthetic recommendation."
                    .into(),
            claim: ConversationClaim::Correction,
            evidence_ids: vec!["user-1".into()],
        };
        assert_eq!(
            conversations::reconcile(
                &s,
                ReconcileChatInput {
                    repository: REPO.into(),
                    revision: corrected["revision"].as_str().unwrap().into(),
                    changes: vec![correction]
                }
            )
            .await
            .unwrap()["complete"],
            true
        );
        let history = SqliteStore::open_read_only(&db)
            .unwrap()
            .lesson_history(&HistoryQuery {
                repository: REPO.into(),
                id: l.id,
                change_id: None,
                limit: 10,
                offset: 0,
            })
            .unwrap();
        assert_eq!(history.changes[0].change.conversation_evidence.len(), 1);
        let progress = conversations::prepare(
            &s,
            PrepareChatInput {
                page: None,
                ..input()
            },
        )
        .unwrap();
        assert_eq!(progress["progress"]["next_cursor"], "synthetic-older-turns");
        stop.cancel();
        task.await.unwrap();
    }
    #[tokio::test]
    async fn failed_publication_does_not_advance_and_pending_drafts_cannot_change() {
        let dir = tempfile::tempdir().unwrap();
        let s = settings(dir.path());
        let v = conversations::prepare(&s, input()).unwrap();
        read(&s, &v);
        assert!(
            conversations::remember(&s, remember(&v, vec![draft()]))
                .await
                .is_err()
        );
        assert!(
            conversations::prepare(
                &s,
                PrepareChatInput {
                    page: None,
                    ..input()
                }
            )
            .unwrap()["progress"]
                .is_null()
        );
        assert_eq!(
            conversations::prepare(&s, input()).unwrap()["already_processed"],
            false
        );
        let path = s
            .cache_dir
            .join("chats")
            .join(v["revision"].as_str().unwrap())
            .join("publication.json");
        dreamer::collector::write_private(
            &path,
            &serde_json::to_vec(&json!({"complete":false,"drafts":[draft()]})).unwrap(),
        )
        .unwrap();
        let mut changed = draft();
        changed.summary =
            "A different pending recommendation cannot replace an uncertain write.".into();
        assert!(
            conversations::remember(&s, remember(&v, vec![changed]))
                .await
                .unwrap_err()
                .to_string()
                .contains("same pending")
        );
    }
    #[tokio::test]
    async fn partial_writes_resume_and_pending_corrections_block_page_completion() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = settings(dir.path());
        let (stop, task, db) = backend(&mut s, dir.path()).await;
        let v = conversations::prepare(&s, input()).unwrap();
        read(&s, &v);
        let mut second = draft();
        second.knowledge_key = Some("fixture.replacement-validation".into());
        second.summary =
            "Validate a replacement identity before removing the existing account access.".into();
        let drafts = vec![draft(), second];
        let lessons =
            conversations::build(&s, REPO, v["revision"].as_str().unwrap(), &drafts).unwrap();
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection.execute_batch(&format!("CREATE TRIGGER fail_second BEFORE INSERT ON lessons WHEN NEW.id='{}' BEGIN SELECT RAISE(ABORT,'synthetic second-write failure'); END;",lessons[1].id)).unwrap();
        assert!(
            conversations::remember(&s, remember(&v, drafts.clone()))
                .await
                .is_err()
        );
        let store = SqliteStore::open_read_only(&db).unwrap();
        assert!(store.get(REPO, &lessons[0].id).unwrap().is_some());
        assert!(store.get(REPO, &lessons[1].id).unwrap().is_none());
        assert_eq!(
            conversations::prepare(&s, input()).unwrap()["already_processed"],
            false
        );
        connection
            .execute_batch("DROP TRIGGER fail_second;")
            .unwrap();
        let resumed = conversations::remember(&s, remember(&v, drafts))
            .await
            .unwrap();
        assert_eq!(resumed["outcomes"][0]["outcome"], "already_known");
        assert_eq!(resumed["outcomes"][1]["outcome"], "inserted");
        let mut newer = input();
        newer.page.as_mut().unwrap()["turns"][0]["startedAt"] = json!("2026-01-02T00:00:00Z");
        newer.page.as_mut().unwrap()["turns"][0]["items"][0]["createdAt"] =
            json!("2026-01-02T00:00:00Z");
        let v = conversations::prepare(&s, newer).unwrap();
        read(&s, &v);
        let corrections: Vec<_> = lessons
            .iter()
            .map(|l| ChatChangeDraft {
                source_id: l.id.clone(),
                action: ChangeAction::Retire,
                target_id: None,
                reason:
                    "The user withdraws this synthetic account recommendation after reassessment."
                        .into(),
                claim: ConversationClaim::Correction,
                evidence_ids: vec!["user-1".into()],
            })
            .collect();
        connection.execute_batch(&format!("CREATE TRIGGER fail_correction BEFORE INSERT ON lesson_changes WHEN NEW.source_id='{}' BEGIN SELECT RAISE(ABORT,'synthetic correction failure'); END;",lessons[1].id)).unwrap();
        assert!(
            conversations::reconcile(
                &s,
                ReconcileChatInput {
                    repository: REPO.into(),
                    revision: v["revision"].as_str().unwrap().into(),
                    changes: corrections
                }
            )
            .await
            .is_err()
        );
        assert_eq!(
            conversations::prepare(&s, {
                let mut page = input();
                page.page.as_mut().unwrap()["turns"][0]["startedAt"] =
                    json!("2026-01-02T00:00:00Z");
                page.page.as_mut().unwrap()["turns"][0]["items"][0]["createdAt"] =
                    json!("2026-01-02T00:00:00Z");
                page
            })
            .unwrap()["pending_reconciliation"],
            true
        );
        assert!(
            conversations::remember(&s, remember(&v, vec![draft()]))
                .await
                .unwrap_err()
                .to_string()
                .contains("pending chat corrections")
        );
        connection
            .execute_batch("DROP TRIGGER fail_correction;")
            .unwrap();
        assert_eq!(
            conversations::reconcile(
                &s,
                ReconcileChatInput {
                    repository: REPO.into(),
                    revision: v["revision"].as_str().unwrap().into(),
                    changes: vec![]
                }
            )
            .await
            .unwrap()["complete"],
            true
        );
        for lesson in lessons {
            assert_eq!(
                store
                    .lesson_history(&HistoryQuery {
                        repository: REPO.into(),
                        id: lesson.id,
                        change_id: None,
                        offset: 0,
                        limit: 10
                    })
                    .unwrap()
                    .changes
                    .len(),
                1
            );
        }
        stop.cancel();
        task.await.unwrap();
    }

    #[test]
    fn expiry_prunes_completed_sources_but_keeps_pending_corrections() {
        let dir = tempfile::tempdir().unwrap();
        let s = settings(dir.path());
        let first = conversations::prepare(&s, input()).unwrap();
        let a = s
            .cache_dir
            .join("chats")
            .join(first["revision"].as_str().unwrap());
        let mut second = input();
        second.page.as_mut().unwrap()["turns"][0]["items"][0]["content"][0]["text"] =
            json!("A second explicit staged migration decision needs independent validation.");
        let second = conversations::prepare(&s, second).unwrap();
        let b = s
            .cache_dir
            .join("chats")
            .join(second["revision"].as_str().unwrap());
        for path in [&a, &b] {
            let mut snapshot: Value =
                serde_json::from_slice(&fs::read(path.join("snapshot.json")).unwrap()).unwrap();
            snapshot["observed_at"] = json!("2020-01-01T00:00:00Z");
            fs::write(
                path.join("snapshot.json"),
                serde_json::to_vec(&snapshot).unwrap(),
            )
            .unwrap();
            dreamer::collector::write_private(
                &path.join("publication.json"),
                br#"{"complete":true}"#,
            )
            .unwrap();
        }
        dreamer::collector::write_private(
            &b.join("reconciliation.json"),
            br#"{"complete":false,"drafts":[]}"#,
        )
        .unwrap();
        let mut third = input();
        third.page.as_mut().unwrap()["turns"][0]["items"][0]["content"][0]["text"] =
            json!("A third staged migration decision remains separate from acceptance tests.");
        conversations::prepare(&s, third).unwrap();
        assert!(!a.exists());
        assert!(b.exists());
    }

    #[tokio::test]
    async fn mac_mcp_chat_tools_publish_and_cli_preview_is_network_free() {
        use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
        let dir = tempfile::tempdir().unwrap();
        let mut s = settings(dir.path());
        let (stop, task, db) = backend(&mut s, dir.path()).await;
        let path = dir.path().join("mac.toml");
        fs::write(&path,format!("repository={REPO:?}\nendpoint={:?}\nwriter_token_file={:?}\ncache_dir={:?}\nlearn_conversations=true\n",s.endpoint,s.writer_token_file,s.cache_dir)).unwrap();
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_dreamer"));
        command.args(["mac", "--settings"]).arg(&path).arg("mcp");
        let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();
        async fn call(
            client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
            name: &'static str,
            args: Value,
        ) -> Value {
            let result = client
                .call_tool(
                    CallToolRequestParams::new(name)
                        .with_arguments(args.as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_ne!(result.is_error, Some(true), "{result:?}");
            result.structured_content.unwrap()
        }
        let v=call(&client,"prepare_chat_learning",json!({"repository":REPO,"thread_id":THREAD,"page":input().page,"observations":input().observations,"sensitive_terms":["Customer Alpha"]})).await;
        call(
            &client,
            "read_chat_evidence",
            json!({"repository":REPO,"revision":v["revision"],"ids":["user-1"]}),
        )
        .await;
        let drafts = dir.path().join("drafts.json");
        fs::write(&drafts, serde_json::to_vec(&vec![draft()]).unwrap()).unwrap();
        let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_dreamer"))
            .args(["mac", "--settings"])
            .arg(&path)
            .args([
                "preview-chat",
                "--repository",
                REPO,
                "--revision",
                v["revision"].as_str().unwrap(),
                "--file",
            ])
            .arg(&drafts)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let preview: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(preview[0]["conversation_evidence"][0]["thread_id"], THREAD);
        assert!(
            SqliteStore::open_read_only(&db)
                .unwrap()
                .get(REPO, preview[0]["id"].as_str().unwrap())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            call(
                &client,
                "remember_chat_lessons",
                json!({"repository":REPO,"revision":v["revision"],"lessons":[draft()]})
            )
            .await["complete"],
            true
        );
        let recall = call(
            &client,
            "recall_knowledge",
            json!({"repository":REPO,"queries":["account access"]}),
        )
        .await;
        assert!(serde_json::to_vec(&recall).unwrap().len() <= 12_000);
        assert_eq!(recall["lessons"][0]["source_chats"][0], THREAD);
        client.cancel().await.unwrap();
        stop.cancel();
        task.await.unwrap();
    }

    #[test]
    fn capture_keeps_source_timestamp_precision_and_unknown_dates_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let s = settings(dir.path());
        let known = conversations::prepare(&s, input()).unwrap();
        let read = conversations::read(
            &s,
            ChatReadInput {
                repository: REPO.into(),
                revision: known["revision"].as_str().unwrap().into(),
                ids: vec!["user-1".into(), "assistant-1".into()],
            },
        )
        .unwrap();
        assert_eq!(
            read["sources"][0]["source_time"]["at"],
            "2026-01-01T00:00:00.000Z"
        );
        assert_eq!(
            read["sources"][0]["source_time"]["basis"],
            "message_timestamp"
        );
        assert_eq!(read["sources"][1]["source_time"]["basis"], "turn_timestamp");
        let mut undated = input();
        undated.page.as_mut().unwrap()["turns"][0]
            .as_object_mut()
            .unwrap()
            .remove("startedAt");
        undated.page.as_mut().unwrap()["turns"][0]["items"][0]
            .as_object_mut()
            .unwrap()
            .remove("createdAt");
        let view = conversations::prepare(&s, undated).unwrap();
        let view = conversations::read(
            &s,
            ChatReadInput {
                repository: REPO.into(),
                revision: view["revision"].as_str().unwrap().into(),
                ids: vec!["user-1".into()],
            },
        )
        .unwrap();
        assert!(view["sources"][0].get("source_time").is_none());
        let mut bad = input();
        bad.page.as_mut().unwrap()["turns"][0]["items"][0]["createdAt"] = json!("bad date");
        assert!(conversations::prepare(&s, bad).is_err());
    }
    #[tokio::test]
    async fn backfill_requires_related_assessment_and_old_corrections_cannot_replace_newer() {
        use dreamer::mac::conversations::{RelatedAssessment, RelatedRelation};
        let dir = tempfile::tempdir().unwrap();
        let mut s = settings(dir.path());
        let (stop, task, db) = backend(&mut s, dir.path()).await;
        let mut newer = input();
        let thread = "00000000-0000-0000-0000-000000000002";
        newer.thread_id = thread.into();
        newer.page.as_mut().unwrap()["thread"]["id"] = json!(thread);
        newer.page.as_mut().unwrap()["turns"][0]["items"][0]["createdAt"] =
            json!("2026-02-01T00:00:00Z");
        let current = conversations::prepare(&s, newer).unwrap();
        read(&s, &current);
        let published = conversations::remember(&s, remember(&current, vec![draft()]))
            .await
            .unwrap();
        let newer_id = published["outcomes"][0]["id"].as_str().unwrap().to_owned();
        let older = conversations::prepare(&s, input()).unwrap();
        read(&s, &older);
        assert!(
            conversations::remember(&s, remember(&older, vec![draft()]))
                .await
                .unwrap_err()
                .to_string()
                .contains("needs assessment")
        );
        let assessed = conversations::assess_lessons(&s, remember(&older, vec![draft()]))
            .await
            .unwrap();
        let a = &assessed["assessments"][0];
        assert_eq!(a["chronology"]["relation"], "older_context");
        let mut reviewed = draft();
        reviewed.assessment_token = Some(a["assessment_token"].as_str().unwrap().into());
        reviewed.related_assessments=vec![RelatedAssessment{id:newer_id.clone(),relation:RelatedRelation::Historical,reason:"This older conversation records historical advice; it does not replace the later source.".into()}];
        let old = conversations::remember(&s, remember(&older, vec![reviewed.clone()]))
            .await
            .unwrap();
        let old_id = old["outcomes"][0]["id"].as_str().unwrap().to_owned();
        assert_eq!(
            conversations::remember(&s, remember(&older, vec![reviewed]))
                .await
                .unwrap()["outcomes"][0]["outcome"],
            "already_known"
        );
        let correction=ChatChangeDraft{source_id:newer_id.clone(),action:ChangeAction::Retire,target_id:None,reason:"The old source contradicts the later migration instruction in this synthetic fixture.".into(),claim:ConversationClaim::Correction,evidence_ids:vec!["user-1".into()]};
        assert!(
            conversations::reconcile(
                &s,
                ReconcileChatInput {
                    repository: REPO.into(),
                    revision: older["revision"].as_str().unwrap().into(),
                    changes: vec![correction]
                }
            )
            .await
            .is_err()
        );
        let store = SqliteStore::open_read_only(&db).unwrap();
        assert_eq!(
            store.get(REPO, &newer_id).unwrap().unwrap().lesson.status,
            dreamer::lesson::LessonStatus::Candidate
        );
        let old = store.get(REPO, &old_id).unwrap().unwrap();
        assert_eq!(old.chronology.unwrap().relation, "older_context");
        stop.cancel();
        task.await.unwrap();
    }

    #[test]
    fn malformed_receipts_and_symlinks_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let s = settings(dir.path());
        let v = conversations::prepare(&s, input()).unwrap();
        let path = s
            .cache_dir
            .join("chats")
            .join(v["revision"].as_str().unwrap())
            .join("publication.json");
        dreamer::collector::write_private(&path, b"malformed").unwrap();
        assert!(conversations::prepare(&s, input()).is_err());
        fs::remove_file(path).unwrap();
        let victim = dir.path().join("victim");
        fs::write(&victim, "preserved").unwrap();
        let read_path = s
            .cache_dir
            .join("chats")
            .join(v["revision"].as_str().unwrap())
            .join("read.json");
        std::os::unix::fs::symlink(&victim, read_path).unwrap();
        assert!(
            conversations::read(
                &s,
                ChatReadInput {
                    repository: REPO.into(),
                    revision: v["revision"].as_str().unwrap().into(),
                    ids: vec!["user-1".into()]
                }
            )
            .is_err()
        );
        assert_eq!(fs::read_to_string(victim).unwrap(), "preserved");
    }
}
