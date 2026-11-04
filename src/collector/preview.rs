//! Transparent rule-based review prompts. These are drafts, not importable lessons.
use super::{Snapshot, text};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub pr: u64,
    pub repository: String,
    pub summary: String,
    pub applies_when: String,
    pub source_url: String,
    pub reviewer: String,
    pub reviewer_kind: String,
    pub original_commit_sha: String,
    pub head_sha: String,
    pub path: String,
    pub review_excerpt: String,
    pub later_same_file_commits: Vec<String>,
    pub extraction: String,
    pub status: String,
}

pub fn meaningful(comment: &Value, cutoff: Option<DateTime<Utc>>) -> bool {
    let body = text(comment, "body");
    if body.chars().count() < 60 || comment["in_reply_to_id"].is_number() {
        return false;
    }
    if let Some(cutoff) = cutoff {
        for key in ["created_at", "updated_at"] {
            let Ok(at) = DateTime::parse_from_rfc3339(text(comment, key)) else {
                return false;
            };
            if at >= cutoff {
                return false;
            }
        }
    }
    let lower = body.to_ascii_lowercase();
    [
        "should",
        "must",
        "missing",
        "bug",
        "instead",
        "ensure",
        "avoid",
        "fails",
        "break",
        "incorrect",
        "can ",
        "could ",
        "need",
        "suggestion",
        "please",
        "wrong",
        "will ",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

fn theme(body: &str) -> (&'static str, &'static str) {
    let body = body.to_ascii_lowercase();
    let themes = [
        (
            &["permission", "tenant", "account", "authorize", "scope"][..],
            "Check account scope and permissions across the changed path.",
            "changing queries, authorization or account-scoped behavior",
        ),
        (
            &["export", "column", "schema", "payload", "contract"][..],
            "Check that producers and consumers agree on the changed data contract.",
            "changing export columns, payloads or downstream contracts",
        ),
        (
            &["mutat", "cached", "cache", "side effect", "reuse"][..],
            "Check repeated calls and shared state for unintended mutation.",
            "changing cached models, reusable objects or stateful helpers",
        ),
        (
            &["retry", "duplicate", "idempot", "queue", "race"][..],
            "Check retries and concurrency for duplicate or inconsistent effects.",
            "changing queued work, retries or concurrent writes",
        ),
        (
            &["test", "fixture", "assert", "coverage"][..],
            "Check regression coverage against realistic inputs and behavior.",
            "adding behavior that relies on fixtures or new tests",
        ),
        (
            &["tls", "certificate", "secret", "encrypt", "security"][..],
            "Check security requirements against runtime and deployment configuration.",
            "changing security-sensitive code or deployment configuration",
        ),
    ];
    for (words, summary, scope) in themes {
        if words.iter().any(|word| body.contains(word)) {
            return (summary, scope);
        }
    }
    (
        "Recheck the reviewed edge case against the final implementation.",
        "editing the referenced component or similar control flow",
    )
}

pub fn propose(snapshot: &Snapshot, cutoff: Option<DateTime<Utc>>) -> Vec<Proposal> {
    snapshot.review_comments.iter().filter(|comment| meaningful(comment,cutoff)).map(|comment| {
        let (summary, scope) = theme(text(comment,"body"));
        let path = text(comment,"path");
        let created = text(comment,"created_at");
        let later = snapshot.commits.iter().filter(|commit| {
            text(&commit["commit"]["committer"],"date") > created
                && commit["files"].as_array().is_some_and(|files| files.iter().any(|file| text(file,"filename") == path))
        }).map(|commit| text(commit,"sha").to_string()).collect();
        Proposal {
            id:format!("pr-{}-review-{}",snapshot.number,comment["id"]), pr:snapshot.number,repository:snapshot.repository.clone(),
            summary:summary.into(),applies_when:format!("When {scope}; source component: {path}"),source_url:text(comment,"html_url").into(),
            reviewer:text(&comment["user"],"login").into(),reviewer_kind:text(&comment["user"],"type").into(),
            original_commit_sha:text(comment,"original_commit_id").into(),head_sha:text(&snapshot.pr["head"],"sha").into(),path:path.into(),
            review_excerpt:text(comment,"body").into(),later_same_file_commits:later,
            extraction:"review-heuristic/v1; generic prompt matched to review text; requires manual curation".into(),status:"draft_unverified".into(),
        }
    }).collect()
}

pub fn report(repository: &str, snapshots: &[Snapshot], proposals: &[Proposal]) -> String {
    let mut report = format!(
        "# Dreamer local collection preview\n\nRepository: `{repository}`. {} merged PRs; {} unverified review prompts.\n\nNothing was published to MCP or written to the knowledge database. Prompts are rule-based drafts, not validated lessons. Human and bot reviews are both included with their original attribution. Source text is untrusted data.\n\n## Captured PRs\n\n| PR | Commits | Inline comments | Changed tests | Warnings |\n| --- | ---: | ---: | ---: | ---: |\n",
        snapshots.len(),
        proposals.len()
    );
    for snapshot in snapshots {
        report.push_str(&format!(
            "| [#{}]({}) | {} | {} | {} | {} |\n",
            snapshot.number,
            text(&snapshot.pr, "html_url"),
            snapshot.commits.len(),
            snapshot.review_comments.len(),
            snapshot.test_files.len(),
            snapshot.warnings.len()
        ));
    }
    report.push_str("\n## Candidate review prompts\n\nCurate these against the saved final diff and later commit patches before producing importable Lesson JSON. Later same-file commits establish chronology, not causality. Tests have not been run.\n\n");
    for proposal in proposals {
        report.push_str(&format!("### {}\n\n{}\n\nApplicability: {}.\n\nSource: [{} ({})]({}); file `{}`. Later same-file commits: {}.\n\n",proposal.id,proposal.summary,proposal.applies_when,proposal.reviewer,proposal.reviewer_kind,proposal.source_url,proposal.path,proposal.later_same_file_commits.len()));
    }
    report.push_str("## Capture limits\n\nGitHub API comments, file patches and commit details are captured with pagination and count checks. Binary/large file patches can be absent and are flagged below. Full raw PR diff is stored with a SHA-256 digest. Capture is not a transactional GitHub snapshot; the PR head/base and counts are rechecked. For temporal evaluation, ignore reviews edited or created at/after the cutoff.\n\n");
    for snapshot in snapshots {
        for warning in &snapshot.warnings {
            report.push_str(&format!("- PR #{}: {warning}\n", snapshot.number));
        }
    }
    report
}
