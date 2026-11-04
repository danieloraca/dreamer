#![cfg(feature = "mac-client")]
use dreamer::{
    collector::{self, Snapshot, github::GitHubApi},
    config::Config,
    mac::{Draft, ReadInput, RememberInput, Settings, evidence},
    mcp::http::{self, Credentials},
    store::{LessonStore, SearchQuery, sqlite::SqliteStore},
};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "cccccccccccccccccccccccccccccccccccccccc";
const READ: &str = "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr";
const WRITE: &str = "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww";
const REPO: &str = "example/project";
struct Fake {
    pr: Value,
    reviews: Vec<Value>,
    comments: Vec<Value>,
}
impl Fake {
    fn new() -> Self {
        Self {
            pr: json!({"number":12,"title":"Synthetic scoped listing","html_url":"https://github.com/example/project/pull/12","state":"open","draft":false,"head":{"sha":B},"base":{"sha":C},"merged_at":null,"changed_files":2,"commits":2,"review_comments":1,"comments":0}),
            reviews: vec![
                json!({"id":30,"state":"APPROVED","commit_id":B,"user":{"login":"reviewer","type":"User"},"submitted_at":"2026-01-01T02:00:00Z","body":"Accepted after scope fix","html_url":"https://github.com/example/project/pull/12#pullrequestreview-30"}),
            ],
            comments: vec![
                json!({"id":20,"body":"Please scope the query to the account before pagination so another account cannot be returned.","path":"src/list.php","html_url":"https://github.com/example/project/pull/12#discussion_r20","original_commit_id":A}),
            ],
        }
    }
}
impl GitHubApi for Fake {
    fn json(&self, path: &str) -> collector::Result<Value> {
        if path == "repos/example/project/pulls/12" {
            return Ok(self.pr.clone());
        }
        if path.contains("/pulls/12/files?") {
            return Ok(
                json!([{"filename":"src/list.php","patch":"-all accounts\n+scope account before page"},{"filename":"tests/ListTest.php","patch":"+assert account boundary on page two"}]),
            );
        }
        if path.contains("/pulls/12/reviews?") {
            return Ok(json!(self.reviews));
        }
        if path.contains("/pulls/12/comments?") {
            return Ok(json!(self.comments));
        }
        if path.contains("/issues/12/comments?") {
            return Ok(json!([]));
        }
        if path.contains("/pulls/12/commits?") {
            return Ok(json!([{"sha":A},{"sha":B}]));
        }
        for sha in [A, B] {
            if path.contains(&format!("/commits/{sha}?")) {
                return Ok(
                    json!({"sha":sha,"commit":{"message":"Synthetic scope fix","committer":{"date":"2026-01-01T01:00:00Z"}},"files":[{"filename":"src/list.php","patch":"+scope account"}]}),
                );
            }
        }
        Err(format!("unexpected fixture path: {path}").into())
    }
    fn diff(&self, _: &str) -> collector::Result<String> {
        Ok("diff --git a/src/list.php b/src/list.php\n+scope account before page\n".into())
    }
}
struct Counted<'a> {
    api: &'a Fake,
    calls: std::sync::Mutex<Vec<String>>,
}
impl<'a> Counted<'a> {
    fn new(api: &'a Fake) -> Self {
        Self {
            api,
            calls: Default::default(),
        }
    }
    fn heavy_calls(&self) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|path| {
                path.contains("/files?") || path.contains("/commits") || path.starts_with("DIFF ")
            })
            .count()
    }
}
impl GitHubApi for Counted<'_> {
    fn json(&self, path: &str) -> collector::Result<Value> {
        self.calls.lock().unwrap().push(path.into());
        self.api.json(path)
    }
    fn diff(&self, path: &str) -> collector::Result<String> {
        self.calls.lock().unwrap().push(format!("DIFF {path}"));
        self.api.diff(path)
    }
}
fn private(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn settings(root: &Path) -> Settings {
    let writer = root.join("writer.env");
    private(&writer, &format!("DREAMER_MCP_WRITE_TOKEN={WRITE}\n"));
    Settings {
        repository: REPO.into(),
        repositories: vec![],
        endpoint: "http://127.0.0.1:1/mcp".into(),
        writer_token_file: writer,
        cache_dir: root.join("cache"),
    }
}
fn prepared(settings: &Settings, api: &Fake) -> (String, Vec<String>) {
    let result =
        evidence::prepare(settings, api, "https://github.com/example/project/pull/12").unwrap();
    let revision = result["revision"].as_str().unwrap().to_string();
    let ids = result["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| {
            s["id"] == "review:20"
                || s["kind"] == "implementation"
                || s["kind"] == "test"
                || s["kind"] == "commit_patch"
        })
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    (revision, ids)
}
fn draft(ids: Vec<String>) -> Draft {
    Draft {
        scope: Default::default(),
        summary: "Apply account scope before paginating list results.".into(),
        applies_when:
            "Changing account-scoped list endpoints or pagination; inspect page boundaries.".into(),
        evidence_ids: ids,
    }
}
#[test]
fn approval_is_current_revision_and_human_aware_without_per_lesson_approval() {
    let mut api = Fake::new();
    assert!(evidence::can_learn(&api.pr, &api.reviews));
    api.reviews[0]["commit_id"] = json!(A);
    assert!(!evidence::can_learn(&api.pr, &api.reviews));
    api.reviews[0]["commit_id"] = json!(B);
    api.reviews[0]["user"]["type"] = json!("Bot");
    assert!(!evidence::can_learn(&api.pr, &api.reviews));
    api.reviews[0]["user"]["type"] = json!("User");
    let mut revoked = api.reviews[0].clone();
    revoked["state"] = json!("DISMISSED");
    revoked["submitted_at"] = json!("2026-01-01T03:00:00Z");
    api.reviews.push(revoked);
    assert!(!evidence::can_learn(&api.pr, &api.reviews));
    api.pr["merged_at"] = json!("2026-01-02T00:00:00Z");
    assert!(evidence::can_learn(&api.pr, &api.reviews));
}
#[test]
fn prepare_open_prs_is_scoped_cached_and_requires_inspected_code_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let settings = settings(dir.path());
    let api = Fake::new();
    for url in [
        "https://untrusted.invalid/example/project/pull/12",
        "https://github.com/another/project/pull/12",
        "https://github.com/example/project/pull/12?redirect=x",
    ] {
        assert!(evidence::prepare(&settings, &api, url).is_err());
    }
    let (revision, ids) = prepared(&settings, &api);
    let (again, _) = prepared(&settings, &api);
    assert_eq!(revision, again);
    assert!(evidence::build_lessons(&settings, &revision, &[draft(ids.clone())]).is_err());
    evidence::read(
        &settings,
        &ReadInput {
            revision: revision.clone(),
            ids: ids.clone(),
            offset: 0,
            kind: None,
            path: None,
        },
    )
    .unwrap();
    let lesson = evidence::build_lessons(&settings, &revision, &[draft(ids.clone())])
        .unwrap()
        .remove(0);
    assert_eq!(lesson.status, dreamer::lesson::LessonStatus::Candidate);
    assert!(lesson.provenance.producer.contains("automatically stored"));
    assert!(
        evidence::build_lessons(
            &settings,
            &revision,
            &[draft(vec!["review:20".into(), "review_summary:30".into()])]
        )
        .is_err()
    );
    assert!(
        evidence::read(
            &settings,
            &ReadInput {
                revision: "../escape".into(),
                ids: vec![],
                offset: 0,
                kind: None,
                path: None,
            }
        )
        .is_err()
    );
    let path = settings.cache_dir.join(&revision).join("snapshot.json");
    let mut snap: Snapshot = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    snap.pr["head"]["sha"] = json!(A);
    fs::write(path, serde_json::to_vec(&snap).unwrap()).unwrap();
    assert!(
        evidence::read(
            &settings,
            &ReadInput {
                revision,
                ids,
                offset: 0,
                kind: None,
                path: None,
            }
        )
        .is_err()
    );
}
#[test]
fn unapproved_pr_can_provide_context_but_not_settled_lessons() {
    let dir = tempfile::tempdir().unwrap();
    let settings = settings(dir.path());
    let mut api = Fake::new();
    api.reviews.clear();
    let (revision, ids) = prepared(&settings, &api);
    evidence::read(
        &settings,
        &ReadInput {
            revision: revision.clone(),
            ids: ids.clone(),
            offset: 0,
            kind: None,
            path: None,
        },
    )
    .unwrap();
    assert!(evidence::build_lessons(&settings, &revision, &[draft(ids)]).is_err());
}
async fn backend(
    root: &Path,
) -> (
    String,
    CancellationToken,
    tokio::task::JoinHandle<()>,
    std::path::PathBuf,
) {
    let dbroot = root.join("pi");
    fs::create_dir(&dbroot).unwrap();
    let database = dbroot.join("knowledge.sqlite3");
    drop(SqliteStore::open(&database).unwrap());
    let config=Config::parse(&format!("data_dir = {:?}\n[github]\nrepository = \"{REPO}\"\nrepositories = [\"{REPO}\", \"example/second\"]\nbatch_size = 10\n[memory]\ndefault_search_limit = 5\n[http]\nlisten = \"127.0.0.1:0\"\nallowed_hosts = [\"127.0.0.1\"]",dbroot)).unwrap();
    let shutdown = CancellationToken::new();
    let router = http::router(
        &config,
        Credentials::new(READ.into(), Some(WRITE.into())).unwrap(),
        shutdown.clone(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let stop = shutdown.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    (endpoint, shutdown, task, database)
}
#[tokio::test]
async fn automatic_remember_persists_deduplicates_and_checks_remote_readback() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = settings(dir.path());
    let (url, stop, task, db) = backend(dir.path()).await;
    settings.endpoint = url;
    let api = Fake::new();
    let (revision, ids) = prepared(&settings, &api);
    evidence::read(
        &settings,
        &ReadInput {
            revision: revision.clone(),
            ids: ids.clone(),
            offset: 0,
            kind: None,
            path: None,
        },
    )
    .unwrap();
    let result = evidence::remember(
        &settings,
        &api,
        RememberInput {
            skip_reason: None,
            revision: revision.clone(),
            lessons: vec![draft(ids.clone())],
        },
    )
    .await
    .unwrap();
    assert_eq!(result["complete"], true);
    assert_eq!(result["outcomes"][0]["outcome"], "inserted");
    assert_eq!(result["manual_approval_required"], false);
    let again = evidence::remember(
        &settings,
        &api,
        RememberInput {
            skip_reason: None,
            revision: revision.clone(),
            lessons: vec![draft(ids)],
        },
    )
    .await
    .unwrap();
    assert_eq!(again["outcomes"][0]["outcome"], "already_known");
    let counted = Counted::new(&api);
    let skip = evidence::prepare(
        &settings,
        &counted,
        "https://github.com/example/project/pull/12",
    )
    .unwrap();
    assert_eq!(skip["already_processed"], true);
    assert!(skip.get("evidence").is_none());
    assert_eq!(counted.heavy_calls(), 0);

    let store = SqliteStore::open_read_only(&db).unwrap();
    let stored = store
        .search(&SearchQuery {
            repository: REPO.into(),
            text: "account".into(),
            limit: 10,
        })
        .unwrap();
    assert_eq!(stored.len(), 1);
    let receipt: Value = serde_json::from_slice(
        &fs::read(settings.cache_dir.join(revision).join("publication.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["complete"], true);
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn changed_review_or_head_stops_before_any_remote_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = settings(dir.path());
    let (url, stop, task, db) = backend(dir.path()).await;
    settings.endpoint = url;
    let mut api = Fake::new();
    let (revision, ids) = prepared(&settings, &api);
    evidence::read(
        &settings,
        &ReadInput {
            revision: revision.clone(),
            ids: ids.clone(),
            offset: 0,
            kind: None,
            path: None,
        },
    )
    .unwrap();
    api.comments[0]["body"] = json!("Dismissed as incorrect after further investigation");
    assert!(
        evidence::remember(
            &settings,
            &api,
            RememberInput {
                skip_reason: None,
                revision: revision.clone(),
                lessons: vec![draft(ids.clone())]
            }
        )
        .await
        .is_err()
    );
    api.pr["head"]["sha"] = json!(A);
    assert!(
        evidence::remember(
            &settings,
            &api,
            RememberInput {
                skip_reason: None,
                revision,
                lessons: vec![draft(ids)]
            }
        )
        .await
        .is_err()
    );
    assert!(
        SqliteStore::open_read_only(&db)
            .unwrap()
            .search(&SearchQuery {
                repository: REPO.into(),
                text: "account".into(),
                limit: 10
            })
            .unwrap()
            .is_empty()
    );
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn mac_stdio_exposes_workflow_tools_without_opening_github_or_pi() {
    use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
    let dir: TempDir = tempfile::tempdir().unwrap();
    let settings = settings(dir.path());
    let path = dir.path().join("mac.toml");
    fs::write(&path,format!("repository = \"{REPO}\"\nendpoint = \"{}\"\nwriter_token_file = {:?}\ncache_dir = {:?}",settings.endpoint,settings.writer_token_file,settings.cache_dir)).unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_dreamer"));
    command.arg("mac").arg("--settings").arg(path).arg("mcp");
    let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 7);
    let error = client
        .call_tool(
            CallToolRequestParams::new("recall_knowledge")
                .with_arguments(json!({"queries":[]}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(error.is_error, Some(true));
    for (name, args) in [
        (
            "recall_knowledge",
            json!({"queries":["account"],"limit":11}),
        ),
        (
            "get_knowledge",
            json!({"repository":"not/allowed","id":"a-lesson"}),
        ),
        ("get_knowledge", json!({"id":""})),
    ] {
        let error = client
            .call_tool(
                CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(error.is_error, Some(true));
    }
    client.cancel().await.unwrap();
    assert!(!settings.cache_dir.exists());
}

struct OtherRepo(Fake);
impl GitHubApi for OtherRepo {
    fn json(&self, path: &str) -> collector::Result<Value> {
        assert!(
            path.contains("repos/example/second/"),
            "Mac used the default scope for a second-repo request: {path}"
        );
        let value = self
            .0
            .json(&path.replace("repos/example/second/", "repos/example/project/"))?;
        Ok(serde_json::from_str(
            &serde_json::to_string(&value)?.replace("example/project", "example/second"),
        )?)
    }
    fn diff(&self, path: &str) -> collector::Result<String> {
        self.0.diff(path)
    }
}
#[tokio::test]
async fn another_repo_keeps_capture_ids_publication_and_recall_in_its_own_scope() {
    use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
    let dir = tempfile::tempdir().unwrap();
    let mut settings = settings(dir.path());
    settings.repositories = vec![REPO.into(), "example/second".into()];
    let (url, stop, task, db) = backend(dir.path()).await;
    settings.endpoint = url;
    let first = prepared(&settings, &Fake::new());
    let api = OtherRepo(Fake::new());
    let meta =
        evidence::prepare(&settings, &api, "https://github.com/EXAMPLE/SECOND/pull/12").unwrap();
    assert_eq!(meta["repository"], "example/second");
    let revision = meta["revision"].as_str().unwrap().to_string();
    assert_ne!(revision, first.0);
    let ids = meta["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["id"] == "review:20" || s["kind"] == "implementation" || s["kind"] == "test")
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    evidence::read(
        &settings,
        &ReadInput {
            revision: revision.clone(),
            ids: ids.clone(),
            offset: 0,
            kind: None,
            path: None,
        },
    )
    .unwrap();
    let lesson = evidence::build_lessons(&settings, &revision, &[draft(ids.clone())])
        .unwrap()
        .remove(0);
    assert_eq!(lesson.repository, "example/second");
    let result = evidence::remember(
        &settings,
        &api,
        RememberInput {
            skip_reason: None,
            revision: revision.clone(),
            lessons: vec![draft(ids.clone())],
        },
    )
    .await
    .unwrap();
    assert_eq!(result["outcomes"][0]["repository"], "example/second");
    let store = SqliteStore::open_read_only(&db).unwrap();
    assert!(store.get(REPO, &lesson.id).unwrap().is_none());
    assert!(store.get("example/second", &lesson.id).unwrap().is_some());
    drop(store);
    let mut writer = SqliteStore::open_existing(&db).unwrap();
    let mut conflicting = lesson.clone();
    conflicting.id = "zz-conflicting-claim".into();
    writer.put_candidate(&conflicting).unwrap();
    writer
        .change_lesson(&dreamer::lifecycle::LessonChange {
            repository: "example/second".into(),
            source_id: lesson.id.clone(),
            target_id: Some(conflicting.id),
            action: dreamer::lifecycle::ChangeAction::Conflict,
            reason: "The two inspected claims disagree for this same account scope.".into(),
            producer: "fixture".into(),
            evidence: lesson.evidence.clone(),
        })
        .unwrap();
    drop(writer);
    let path = dir.path().join("multi-mac.toml");
    fs::write(&path,format!("repository = \"{REPO}\"\nrepositories = [\"{REPO}\", \"example/second\"]\nendpoint = \"{}\"\nwriter_token_file = {:?}\ncache_dir = {:?}",settings.endpoint,settings.writer_token_file,settings.cache_dir)).unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_dreamer"));
    command.arg("mac").arg("--settings").arg(path).arg("mcp");
    let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();
    let listing = client
        .call_tool(
            CallToolRequestParams::new("list_repositories")
                .with_arguments(json!({}).as_object().unwrap().clone()),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(listing["repositories"], json!([REPO, "example/second"]));
    let recall = client
        .call_tool(
            CallToolRequestParams::new("recall_knowledge").with_arguments(
                json!({"repository":"example/second","queries":["account"]})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(recall.is_error, Some(true));
    let recall = recall.structured_content.unwrap();
    assert_eq!(recall["repository"], "example/second");
    assert_eq!(recall["lessons"][0]["lesson"]["id"], lesson.id);
    assert_eq!(recall["lessons"][0]["lifecycle"]["conflict_count"], 1);
    assert!(recall["lessons"][0]["lesson"].get("evidence").is_none());
    assert_eq!(
        recall["lessons"][0]["evidence_count"],
        lesson.evidence.len()
    );
    let full = client
        .call_tool(
            CallToolRequestParams::new("get_knowledge").with_arguments(
                json!({"repository":"example/second","id":lesson.id})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(full.is_error, Some(true));
    let full = full.structured_content.unwrap();
    assert_eq!(
        full["history"]["changes"][0]["change"]["action"],
        "conflict"
    );
    assert_eq!(
        full["lesson"]["lesson"],
        serde_json::to_value(&lesson).unwrap()
    );
    let missing = client
        .call_tool(
            CallToolRequestParams::new("get_knowledge").with_arguments(
                json!({"repository":REPO,"id":lesson.id})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(missing.structured_content.unwrap()["found"], false);

    let default = client
        .call_tool(
            CallToolRequestParams::new("recall_knowledge")
                .with_arguments(json!({"queries":["account"]}).as_object().unwrap().clone()),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(default["repository"], REPO);
    assert!(default["lessons"].as_array().unwrap().is_empty());
    client.cancel().await.unwrap();
    settings.repositories = vec![REPO.into()];
    assert!(
        evidence::read(
            &settings,
            &ReadInput {
                revision,
                ids,
                offset: 0,
                kind: None,
                path: None
            }
        )
        .is_err()
    );
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn completed_evidence_skips_heavy_capture_but_new_feedback_and_force_reopen_it() {
    let dir = tempfile::tempdir().unwrap();
    let settings = settings(dir.path()); // Unreachable Pi: an empty completion must not connect.
    let mut api = Fake::new();
    let (revision, ids) = prepared(&settings, &api);
    let reason = "Inspected the review and code; no additional reusable lesson was supported.";
    assert!(
        evidence::remember(
            &settings,
            &api,
            RememberInput {
                revision: revision.clone(),
                lessons: vec![],
                skip_reason: Some(reason.into())
            }
        )
        .await
        .is_err()
    );
    evidence::read(
        &settings,
        &ReadInput {
            revision: revision.clone(),
            ids,
            offset: 0,
            kind: None,
            path: None,
        },
    )
    .unwrap();
    let done = evidence::remember(
        &settings,
        &api,
        RememberInput {
            revision: revision.clone(),
            lessons: vec![],
            skip_reason: Some(reason.into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(done["complete"], true);
    assert!(done["outcomes"].as_array().unwrap().is_empty());
    // Volatile GitHub metadata should not spend another model extraction.
    api.pr["updated_at"] = json!("2026-02-02T00:00:00Z");
    let counted = Counted::new(&api);
    let skipped = evidence::prepare(
        &settings,
        &counted,
        "https://github.com/example/project/pull/12",
    )
    .unwrap();
    assert_eq!(skipped["already_processed"], true);
    assert_eq!(counted.heavy_calls(), 0);
    assert!(serde_json::to_vec(&skipped).unwrap().len() < 700);
    let forced = evidence::prepare_with_force(
        &settings,
        &counted,
        "https://github.com/example/project/pull/12",
        true,
    )
    .unwrap();
    assert_eq!(forced["already_processed"], false);
    assert!(counted.heavy_calls() > 0);
    // Edit existing feedback without changing the head or any counts.
    api.comments[0]["body"] = json!("New finding: the old account scope advice is incomplete.");
    let counted = Counted::new(&api);
    let reopened = evidence::prepare(
        &settings,
        &counted,
        "https://github.com/example/project/pull/12",
    )
    .unwrap();
    assert_eq!(reopened["already_processed"], false);
    assert_ne!(reopened["revision"], revision);
    assert!(counted.heavy_calls() > 0);
    assert!(
        evidence::remember(
            &settings,
            &api,
            RememberInput {
                revision,
                lessons: vec![],
                skip_reason: Some(reason.into())
            }
        )
        .await
        .is_err()
    );
    api.pr["head"]["sha"] = json!(A);
    let changed_head = evidence::prepare(
        &settings,
        &api,
        "https://github.com/example/project/pull/12",
    )
    .unwrap();
    assert_eq!(changed_head["already_processed"], false);
    assert_eq!(changed_head["can_learn"], false);
}

#[tokio::test]
async fn partial_receipt_never_skips_and_exact_legacy_completion_is_reindexed() {
    let dir = tempfile::tempdir().unwrap();
    let settings = settings(dir.path());
    let api = Fake::new();
    let (revision, ids) = prepared(&settings, &api);
    evidence::read(
        &settings,
        &ReadInput {
            revision: revision.clone(),
            ids,
            offset: 0,
            kind: None,
            path: None,
        },
    )
    .unwrap();
    evidence::remember(
        &settings,
        &api,
        RememberInput {
            revision: revision.clone(),
            lessons: vec![],
            skip_reason: Some(
                "No new knowledge after inspecting the review and resulting implementation.".into(),
            ),
        },
    )
    .await
    .unwrap();
    let receipt = settings.cache_dir.join(&revision).join("publication.json");
    // The completion index cannot turn a partial/uncertain publication into success.
    fs::write(&receipt, serde_json::to_vec(&json!({"complete":false,"revision":revision,"in_flight":"lesson-pending","outcomes":[]})).unwrap()).unwrap();
    let counted = Counted::new(&api);
    assert_eq!(
        evidence::prepare(
            &settings,
            &counted,
            "https://github.com/example/project/pull/12"
        )
        .unwrap()["already_processed"],
        false
    );
    assert!(counted.heavy_calls() > 0);
    // Emulate a pre-index successful receipt: recognize only an exact cached capture.
    for entry in fs::read_dir(settings.cache_dir.join("completed")).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::write(&receipt,serde_json::to_vec(&json!({"complete":true,"revision":revision,"outcomes":[{"id":"learn-legacy","outcome":"already_known"}],"manual_approval_required":false})).unwrap()).unwrap();
    assert_eq!(
        evidence::prepare(
            &settings,
            &api,
            "https://github.com/example/project/pull/12"
        )
        .unwrap()["already_processed"],
        true
    );
    let counted = Counted::new(&api);
    assert_eq!(
        evidence::prepare(
            &settings,
            &counted,
            "https://github.com/example/project/pull/12"
        )
        .unwrap()["already_processed"],
        true
    );
    assert_eq!(counted.heavy_calls(), 0);
    // Losing approval at the same head invalidates completion and prevents empty finish.
    let mut unapproved = Fake::new();
    unapproved.reviews[0]["state"] = json!("DISMISSED");
    let reopened = evidence::prepare(
        &settings,
        &unapproved,
        "https://github.com/example/project/pull/12",
    )
    .unwrap();
    assert_eq!(reopened["already_processed"], false);
    assert_eq!(reopened["can_learn"], false);
}

fn reconcile_input(revision: &str, ids: &[String]) -> dreamer::mac::reconciliation::ReconcileInput {
    use dreamer::{
        lifecycle::ChangeAction,
        mac::reconciliation::{ChangeDraft, ReconcileInput},
    };
    ReconcileInput {revision:revision.into(),changes:vec![
        ChangeDraft {source_id:"earlier".into(),action:ChangeAction::Supersede,target_id:Some("retained".into()),reason:"The inspected review and resulting account-scoped query replace the earlier advice.".into(),evidence_ids:ids.into()},
        ChangeDraft {source_id:"unsupported".into(),action:ChangeAction::Retire,target_id:None,reason:"The reviewed final implementation disproves the overly broad earlier assertion.".into(),evidence_ids:ids.into()},
    ]}
}
#[tokio::test]
async fn mac_reconciliation_resumes_partial_changes_and_never_hides_pending_work() {
    use dreamer::{lifecycle::HistoryQuery, mac::reconciliation::reconcile};
    let dir = tempfile::tempdir().unwrap();
    let mut settings = settings(dir.path());
    let (url, stop, task, db) = backend(dir.path()).await;
    settings.endpoint = url;
    let mut api = Fake::new();
    let (revision, ids) = prepared(&settings, &api);
    assert!(
        reconcile(&settings, &api, reconcile_input(&revision, &ids))
            .await
            .is_err()
    ); // No inspected sources.
    evidence::read(
        &settings,
        &ReadInput {
            revision: revision.clone(),
            ids: ids.clone(),
            offset: 0,
            kind: None,
            path: None,
        },
    )
    .unwrap();
    let seed = evidence::build_lessons(&settings, &revision, &[draft(ids.clone())])
        .unwrap()
        .remove(0);
    let mut store = SqliteStore::open_existing(&db).unwrap();
    for id in ["earlier", "unsupported", "retained"] {
        let mut seed = seed.clone();
        seed.id = id.into();
        store.put_candidate(&seed).unwrap();
    }
    drop(store);
    evidence::remember(&settings,&api,RememberInput {revision:revision.clone(),lessons:vec![],skip_reason:Some("Existing knowledge covers this PR; reconcile its older advice using the inspected evidence.".into())}).await.unwrap();
    let mut invalid = reconcile_input(&revision, &ids);
    invalid.changes[0].target_id = Some("not-a-stored-lesson".into());
    assert!(reconcile(&settings, &api, invalid).await.is_err());
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection.execute_batch("CREATE TRIGGER fixture_reconcile_failure BEFORE INSERT ON lesson_changes WHEN NEW.source_id='unsupported' BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert!(
        reconcile(&settings, &api, reconcile_input(&revision, &ids))
            .await
            .is_err()
    );
    let store = SqliteStore::open_read_only(&db).unwrap();
    assert_eq!(
        store.get(REPO, "earlier").unwrap().unwrap().lesson.status,
        dreamer::lesson::LessonStatus::Deprecated
    );
    assert_eq!(
        store
            .get(REPO, "unsupported")
            .unwrap()
            .unwrap()
            .lesson
            .status,
        dreamer::lesson::LessonStatus::Candidate
    );
    drop(store);
    let pending = evidence::prepare(
        &settings,
        &api,
        "https://github.com/example/project/pull/12",
    )
    .unwrap();
    assert_eq!(pending["already_processed"], false);
    assert_eq!(pending["pending_reconciliation"]["draft_count"], 2);
    let receipt: Value = serde_json::from_slice(
        &fs::read(
            settings
                .cache_dir
                .join(&revision)
                .join("reconciliation.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["complete"], false);
    let mut different = reconcile_input(&revision, &ids);
    different.changes[0].reason =
        "A changed reason cannot conceal an uncertain previous operation.".into();
    assert!(reconcile(&settings, &api, different).await.is_err());
    connection
        .execute_batch("DROP TRIGGER fixture_reconcile_failure;")
        .unwrap();
    let finished = reconcile(
        &settings,
        &api,
        dreamer::mac::reconciliation::ReconcileInput {
            revision: revision.clone(),
            changes: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!(finished["learning_complete"], true);
    assert_eq!(finished["outcomes"][0]["outcome"], "already_applied");
    assert_eq!(finished["outcomes"][1]["outcome"], "applied");
    assert_eq!(
        evidence::prepare(
            &settings,
            &api,
            "https://github.com/example/project/pull/12"
        )
        .unwrap()["already_processed"],
        true
    );
    assert_eq!(
        reconcile(&settings, &api, reconcile_input(&revision, &ids))
            .await
            .unwrap()["outcomes"][1]["outcome"],
        "already_applied"
    );
    let store = SqliteStore::open_read_only(&db).unwrap();
    let events = store
        .lesson_history(&HistoryQuery {
            repository: REPO.into(),
            id: "earlier".into(),
            change_id: None,
            limit: 20,
            offset: 0,
        })
        .unwrap();
    assert_eq!(events.changes.len(), 1);
    assert_eq!(events.changes[0].change.evidence.len(), ids.len());
    drop(store);
    // Successful lifecycle work cannot overwrite an incomplete candidate receipt.
    fs::write(settings.cache_dir.join(&revision).join("publication.json"),serde_json::to_vec(&json!({"complete":false,"revision":revision,"outcomes":[],"in_flight":"unconfirmed-candidate"})).unwrap()).unwrap();
    assert_eq!(
        reconcile(&settings, &api, reconcile_input(&revision, &ids))
            .await
            .unwrap()["learning_complete"],
        false
    );
    assert_eq!(
        evidence::prepare(
            &settings,
            &api,
            "https://github.com/example/project/pull/12"
        )
        .unwrap()["already_processed"],
        false
    );
    // Even a retry must revalidate current review content before any Pi action.
    api.comments[0]["body"] = json!("Later feedback changes the claimed correction.");
    assert!(
        reconcile(&settings, &api, reconcile_input(&revision, &ids))
            .await
            .is_err()
    );
    stop.cancel();
    task.await.unwrap();
}

async fn recall_command(settings: &Settings, args: &[&str]) -> Value {
    let path = settings
        .cache_dir
        .parent()
        .unwrap()
        .join("recall-settings.toml");
    fs::write(
        &path,
        format!(
            "repository = {:?}\nendpoint = {:?}\nwriter_token_file = {:?}\ncache_dir = {:?}",
            settings.repository, settings.endpoint, settings.writer_token_file, settings.cache_dir
        ),
    )
    .unwrap();
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .arg("mac")
        .arg("--settings")
        .arg(path)
        .arg("recall")
        .args(args)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[tokio::test]
async fn scoped_agent_learning_keeps_identity_and_pinned_sources_and_supports_context_recall() {
    use dreamer::retrieval::LessonScope;
    let dir = tempfile::tempdir().unwrap();
    let mut settings = settings(dir.path());
    let (url, stop, task, db) = backend(dir.path()).await;
    settings.endpoint = url;
    let api = Fake::new();
    let (revision, ids) = prepared(&settings, &api);
    evidence::read(
        &settings,
        &ReadInput {
            revision: revision.clone(),
            ids: ids.clone(),
            offset: 0,
            kind: None,
            path: None,
        },
    )
    .unwrap();
    let original = evidence::build_lessons(&settings, &revision, &[draft(ids.clone())])
        .unwrap()
        .remove(0);
    let mut scoped = draft(ids.clone());
    scoped.scope = LessonScope {
        components: vec!["Lists".into()],
        paths: vec!["src/".into()],
        excluded_terms: vec!["public endpoints".into()],
    };
    let candidate = evidence::build_lessons(&settings, &revision, &[scoped])
        .unwrap()
        .remove(0);
    assert_ne!(candidate.id, original.id);
    assert_eq!(candidate.scope.components, vec!["lists"]);
    assert_eq!(candidate.evidence, original.evidence);
    let mut scoped = draft(ids.clone());
    scoped.scope = candidate.scope.clone();
    evidence::remember(
        &settings,
        &api,
        RememberInput {
            revision: revision.clone(),
            lessons: vec![scoped],
            skip_reason: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        SqliteStore::open_read_only(&db)
            .unwrap()
            .get(REPO, &candidate.id)
            .unwrap()
            .unwrap()
            .lesson
            .scope,
        candidate.scope
    );
    let recall = recall_command(
        &settings,
        &["--query", "opaque-task", "--path", "src/list.php"],
    )
    .await;
    assert_eq!(recall["retrieval_mode"], "ranked");
    assert_eq!(recall["lessons"][0]["lesson"]["id"], candidate.id);
    assert!(recall["lessons"][0]["relevance"]["score"].as_u64().unwrap() > 0);
    assert!(serde_json::to_vec(&recall).unwrap().len() <= 12000);
    let excluded = recall_command(
        &settings,
        &["--query", "account", "--query", "public endpoints"],
    )
    .await;
    assert!(excluded["lessons"].as_array().unwrap().is_empty());
    stop.cancel();
    task.await.unwrap();
}

#[derive(Clone)]
struct LegacyPi {
    database: std::path::PathBuf,
}
impl rmcp::ServerHandler for LegacyPi {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
    }
    async fn list_tools(
        &self,
        _: Option<rmcp::model::PaginatedRequestParams>,
        _: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListToolsResult::with_all_items(
            ["search_lessons", "get_lesson"]
                .into_iter()
                .map(|name| {
                    rmcp::model::Tool::new(
                        name,
                        "Synthetic old Pi contract",
                        json!({"type":"object"}).as_object().unwrap().clone(),
                    )
                })
                .collect(),
        ))
    }
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        _: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, rmcp::ErrorData> {
        let args = Value::Object(request.arguments.unwrap_or_default());
        assert_eq!(args["repository"], REPO);
        let store = SqliteStore::open_read_only(&self.database).unwrap();
        let value = match request.name.as_ref() {
            "search_lessons" => {
                json!({"repository":REPO,"lessons":store.search(&SearchQuery {repository:REPO.into(),text:args["query"].as_str().unwrap().into(),limit:args["limit"].as_u64().unwrap() as usize}).unwrap()})
            }
            "get_lesson" => {
                let item = store.get(REPO, args["id"].as_str().unwrap()).unwrap();
                json!({"found":item.is_some(),"lesson":item})
            }
            _ => panic!("Old Pi must not receive new ranked/scoped-write operations"),
        };
        Ok(rmcp::model::CallToolResult::structured(value).into())
    }
}
#[tokio::test]
async fn older_pi_fallback_uses_the_same_exclusion_guard_and_rejects_scoped_writes_before_mutation()
{
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut settings = settings(dir.path());
    let db = dir.path().join("legacy.sqlite3");
    let mut store = SqliteStore::open(&db).unwrap();
    let mut good: dreamer::lesson::Lesson =
        serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap();
    good.repository = REPO.into();
    good.id = "b-export".into();
    good.summary = "Preserve export configuration during cloning.".into();
    good.applies_when = "When cloning exports.".into();
    store.put_candidate(&good).unwrap();
    let mut irrelevant = good.clone();
    irrelevant.id = "a-locks".into();
    irrelevant.summary = "Lock entity mutations before editing.".into();
    irrelevant.applies_when = "When editing entity hierarchies; not for exports.".into();
    store.put_candidate(&irrelevant).unwrap();
    drop(store);
    let stop = CancellationToken::new();
    let database = db.clone();
    let service = StreamableHttpService::new(
        move || {
            Ok(LegacyPi {
                database: database.clone(),
            })
        },
        std::sync::Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_json_response(true)
            .with_cancellation_token(stop.child_token()),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    settings.endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let shutdown = stop.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    let recall = recall_command(&settings, &["--query", "export"]).await;
    assert_eq!(recall["retrieval_mode"], "legacy_fallback");
    assert_eq!(recall["lessons"].as_array().unwrap().len(), 1);
    assert_eq!(recall["lessons"][0]["lesson"]["id"], good.id);
    let api = Fake::new();
    let (revision, ids) = prepared(&settings, &api);
    evidence::read(
        &settings,
        &ReadInput {
            revision: revision.clone(),
            ids: ids.clone(),
            offset: 0,
            kind: None,
            path: None,
        },
    )
    .unwrap();
    let mut scoped = draft(ids);
    scoped.scope.components = vec!["lists".into()];
    let error = evidence::remember(
        &settings,
        &api,
        RememberInput {
            revision,
            lessons: vec![scoped],
            skip_reason: None,
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("Pi upgrade required"));
    stop.cancel();
    task.await.unwrap();
}
