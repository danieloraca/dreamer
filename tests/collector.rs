use chrono::{TimeZone, Utc};
use dreamer::collector::{
    self, CollectOptions,
    github::{GitHubApi, pages},
    preview,
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap};

const REPO: &str = "example/project";
const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "cccccccccccccccccccccccccccccccccccccccc";

struct FakeApi {
    values: BTreeMap<String, Value>,
    calls: RefCell<Vec<String>>,
}
impl GitHubApi for FakeApi {
    fn json(&self, path: &str) -> collector::Result<Value> {
        self.calls.borrow_mut().push(path.into());
        if path.starts_with("search/issues?") {
            return Ok(json!({"incomplete_results":false,"items":[{"number":12}]}));
        }
        self.values
            .get(path)
            .cloned()
            .ok_or_else(|| format!("unexpected fixture GET {path}").into())
    }
    fn diff(&self, path: &str) -> collector::Result<String> {
        self.calls.borrow_mut().push(path.into());
        Ok("diff --git a/src/service.php b/src/service.php\n+validated input\n".into())
    }
}

fn api() -> FakeApi {
    let pr = json!({"number":12,"title":"Synthetic input validation fix","html_url":"https://github.com/example/project/pull/12",
        "head":{"sha":B},"base":{"sha":C},"created_at":"2026-01-01T00:00:00Z","merged_at":"2026-01-02T00:00:00Z",
        "changed_files":2,"commits":2,"review_comments":1,"comments":0});
    let comment = json!({"id":21,"body":"Please ensure this branch validates the account scope before executing the query; otherwise another account could be returned.",
        "user":{"login":"fixture-reviewer","type":"User"},"path":"src/service.php","html_url":"https://github.com/example/project/pull/12#discussion_r21",
        "original_commit_id":A,"created_at":"2026-01-01T01:00:00Z","updated_at":"2026-01-01T01:00:00Z"});
    let mut values = BTreeMap::from([
        ("repos/example/project/pulls/12".into(), pr),
        (
            "repos/example/project/pulls/12/files?per_page=100&page=1".into(),
            json!([{"filename":"src/service.php","patch":"+validated"},{"filename":"tests/ServiceTest.php","patch":"+assert"}]),
        ),
        (
            "repos/example/project/pulls/12/comments?per_page=100&page=1".into(),
            json!([comment]),
        ),
        (
            "repos/example/project/pulls/12/reviews?per_page=100&page=1".into(),
            json!([{"state":"APPROVED","body":"Synthetic review"}]),
        ),
        (
            "repos/example/project/issues/12/comments?per_page=100&page=1".into(),
            json!([]),
        ),
        (
            "repos/example/project/pulls/12/commits?per_page=100&page=1".into(),
            json!([{"sha":A},{"sha":B}]),
        ),
    ]);
    for (sha, date) in [(A, "2026-01-01T00:00:00Z"), (B, "2026-01-01T02:00:00Z")] {
        values.insert(format!("repos/example/project/commits/{sha}?per_page=100&page=1"),json!({"sha":sha,"commit":{"committer":{"date":date},"message":"synthetic follow-up"},"files":[{"filename":"src/service.php","patch":"+scope check"}]}));
    }
    FakeApi {
        values,
        calls: RefCell::new(vec![]),
    }
}

fn options(path: std::path::PathBuf, count: usize) -> CollectOptions {
    CollectOptions {
        repository: REPO.into(),
        count,
        scan_limit: 10,
        holdout: None,
        before: Some(Utc.with_ymd_and_hms(2026, 1, 3, 0, 0, 0).unwrap()),
        output: path,
    }
}

#[test]
fn dry_run_captures_evidence_and_proposals_without_a_database_or_publication() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("batch");
    let report = collector::collect(&api(), &options(output.clone(), 1)).unwrap();
    assert!(report.is_file());
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["complete"], true);
    assert_eq!(manifest["published"], false);
    assert_eq!(manifest["selected_count"], 1);
    let snapshot: collector::Snapshot =
        serde_json::from_slice(&std::fs::read(output.join("pr-12/snapshot.json")).unwrap())
            .unwrap();
    assert_eq!(snapshot.commits.len(), 2);
    assert_eq!(snapshot.test_files, ["tests/ServiceTest.php"]);
    assert_eq!(
        snapshot.diff_sha256,
        collector::digest(&std::fs::read(output.join("pr-12/diff.patch")).unwrap())
    );
    let proposals: Vec<preview::Proposal> =
        serde_json::from_slice(&std::fs::read(output.join("proposals.json")).unwrap()).unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].later_same_file_commits, [B]);
    assert_eq!(proposals[0].status, "draft_unverified");
    assert_eq!(
        std::fs::read_to_string(output.join(".gitignore")).unwrap(),
        "*\n"
    );
    assert!(!output.join("knowledge.sqlite3").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&report).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn an_existing_preview_is_never_overwritten_or_fetched_again() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("batch");
    std::fs::create_dir(&output).unwrap();
    std::fs::write(output.join("keep"), "original").unwrap();
    let api = api();
    assert!(collector::collect(&api, &options(output.clone(), 1)).is_err());
    assert!(api.calls.borrow().is_empty());
    assert_eq!(
        std::fs::read_to_string(output.join("keep")).unwrap(),
        "original"
    );
}

#[test]
fn too_few_eligible_prs_mark_the_batch_incomplete() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("batch");
    assert!(collector::collect(&api(), &options(output.clone(), 2)).is_err());
    assert!(output.join("incomplete.json").is_file());
    assert!(!output.join("manifest.json").exists());
}

#[test]
fn pagination_detects_truncation_and_handles_multiple_pages() {
    let first: Vec<Value> = (0..100).map(|id| json!({"id":id})).collect();
    let mut api = FakeApi {
        values: BTreeMap::from([
            ("comments?per_page=100&page=1".into(), json!(first)),
            ("comments?per_page=100&page=2".into(), json!([{"id":100}])),
        ]),
        calls: RefCell::new(vec![]),
    };
    assert_eq!(pages(&api, "comments", Some(101)).unwrap().len(), 101);
    api.values
        .insert("comments?per_page=100&page=2".into(), json!([]));
    assert!(pages(&api, "comments", Some(101)).is_err());
}

#[test]
fn revision_changes_and_api_limits_cannot_be_silently_accepted() {
    let mut api = api();
    let original = api.values["repos/example/project/pulls/12"].clone();
    api.values
        .get_mut("repos/example/project/pulls/12")
        .unwrap()["head"]["sha"] = json!(C);
    assert!(collector::capture(&api, REPO, 12, original.clone()).is_err());
    let mut oversized = original;
    oversized["commits"] = json!(251);
    let before = api.calls.borrow().len();
    assert!(collector::capture(&api, REPO, 12, oversized).is_err());
    assert_eq!(before, api.calls.borrow().len());
}

#[test]
fn temporal_filter_excludes_later_edits_and_replies_without_trusting_review_text() {
    let mut comment =
        api().values["repos/example/project/pulls/12/comments?per_page=100&page=1"][0].clone();
    let cutoff = Some(Utc.with_ymd_and_hms(2026, 1, 3, 0, 0, 0).unwrap());
    assert!(preview::meaningful(&comment, cutoff));
    comment["updated_at"] = json!("2026-01-04T00:00:00Z");
    assert!(!preview::meaningful(&comment, cutoff));
    comment["updated_at"] = json!("2026-01-01T01:00:00Z");
    comment["in_reply_to_id"] = json!(20);
    assert!(!preview::meaningful(&comment, cutoff));
}

#[test]
fn collect_cli_does_not_require_service_config_and_rejects_invalid_repo_before_io() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("batch");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .args(["collect", "--repository", "invalid", "--output"])
        .arg(&output)
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("owner/repository"));
    assert!(!output.exists());
}

#[test]
fn a_preview_write_failure_never_leaves_a_complete_manifest() {
    struct Collision(FakeApi, std::path::PathBuf);
    impl GitHubApi for Collision {
        fn json(&self, path: &str) -> collector::Result<Value> {
            if path.starts_with("search/issues?") {
                std::fs::write(&self.1, "operator's existing report")?;
            }
            self.0.json(path)
        }
        fn diff(&self, path: &str) -> collector::Result<String> {
            self.0.diff(path)
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("batch");
    let api = Collision(api(), output.join("report.md"));
    assert!(collector::collect(&api, &options(output.clone(), 1)).is_err());
    assert!(!output.join("manifest.json").exists());
    assert_eq!(
        std::fs::read_to_string(output.join("report.md")).unwrap(),
        "operator's existing report"
    );
}
