//! Manual Mac-side collection. Always local, read-only upstream, never publishes.
pub mod github;
pub mod preview;

use crate::lesson::validate_repository;
use chrono::{DateTime, Utc};
use github::{GitHubApi, pages};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub repository: String,
    pub number: u64,
    pub captured_at: DateTime<Utc>,
    pub pr: Value,
    pub files: Vec<Value>,
    pub review_comments: Vec<Value>,
    pub reviews: Vec<Value>,
    pub discussion_comments: Vec<Value>,
    pub commits: Vec<Value>,
    pub test_files: Vec<String>,
    pub warnings: Vec<String>,
    pub diff_sha256: String,
}

pub struct CollectOptions {
    pub repository: String,
    pub count: usize,
    pub scan_limit: usize,
    pub holdout: Option<u64>,
    pub before: Option<DateTime<Utc>>,
    pub output: PathBuf,
}

pub fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

pub fn is_test(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.starts_with("tests/")
        || path.starts_with("test/")
        || path.contains("/tests/")
        || path.contains(".test.")
        || path.contains(".spec.")
        || path.ends_with("test.php")
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn write_private(path: &Path, contents: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(contents)?;
    Ok(())
}

pub fn private_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}

fn count(pr: &Value, field: &str) -> Result<usize> {
    Ok(pr[field]
        .as_u64()
        .ok_or_else(|| format!("missing PR {field} count"))? as usize)
}

fn sha(value: &str) -> Result<&str> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("GitHub returned an invalid commit SHA".into());
    }
    Ok(value)
}

fn commit(api: &impl GitHubApi, root: &str, id: &str) -> Result<Value> {
    sha(id)?;
    let mut result = api.json(&format!("{root}/commits/{id}?per_page=100&page=1"))?;
    if result["sha"].as_str() != Some(id) {
        return Err("commit identity mismatch".into());
    }
    let mut files = result["files"]
        .as_array()
        .ok_or("commit response has no files")?
        .clone();
    let mut last_count = files.len();
    let mut page = 2;
    while last_count == 100 {
        if page > 30 {
            return Err("commit file list hit GitHub's 3000-file limit".into());
        }
        let next = api.json(&format!("{root}/commits/{id}?per_page=100&page={page}"))?;
        if next["sha"].as_str() != Some(id) {
            return Err("commit pagination identity mismatch".into());
        }
        let more = next["files"].as_array().ok_or("commit page has no files")?;
        last_count = more.len();
        files.extend(more.iter().cloned());
        page += 1;
    }
    result["files"] = json!(files);
    Ok(result)
}

pub fn capture(
    api: &impl GitHubApi,
    repository: &str,
    number: u64,
    pr: Value,
) -> Result<(Snapshot, String)> {
    if pr["merged_at"].as_str().is_none() {
        return Err("only a matching, merged pull request can enter the training batch".into());
    }
    capture_pr(api, repository, number, pr)
}

/// Capture a specific open/approved/merged PR for an agent. This does not approve it.
pub fn capture_pr(
    api: &impl GitHubApi,
    repository: &str,
    number: u64,
    pr: Value,
) -> Result<(Snapshot, String)> {
    crate::lesson::validate_repository(repository)?;
    let root = format!("repos/{repository}");
    let pull = format!("{root}/pulls/{number}");
    if pr["number"].as_u64() != Some(number) {
        return Err("GitHub returned a different pull request".into());
    }
    sha(text(&pr["head"], "sha"))?;
    sha(text(&pr["base"], "sha"))?;
    if count(&pr, "changed_files")? > 3000 || count(&pr, "commits")? > 250 {
        return Err("PR exceeds GitHub's complete pull-files/commits API limits".into());
    }
    let files = pages(
        api,
        &format!("{pull}/files"),
        Some(count(&pr, "changed_files")?),
    )?;
    let review_comments = pages(
        api,
        &format!("{pull}/comments"),
        Some(count(&pr, "review_comments")?),
    )?;
    let reviews = pages(api, &format!("{pull}/reviews"), None)?;
    let discussion_comments = pages(
        api,
        &format!("{root}/issues/{number}/comments"),
        Some(count(&pr, "comments")?),
    )?;
    let commit_refs = pages(
        api,
        &format!("{pull}/commits"),
        Some(count(&pr, "commits")?),
    )?;
    let mut commits = Vec::new();
    for item in commit_refs {
        commits.push(commit(api, &root, text(&item, "sha"))?);
    }
    let diff = api.diff(&pull)?;
    if diff.trim().is_empty() && !files.is_empty() {
        return Err("GitHub returned an empty PR diff".into());
    }
    let current = api.json(&pull)?;
    for pointer in [
        "/head/sha",
        "/base/sha",
        "/comments",
        "/review_comments",
        "/commits",
        "/changed_files",
    ] {
        if current.pointer(pointer) != pr.pointer(pointer) {
            return Err("PR changed while collecting; rerun into a new batch directory".into());
        }
    }
    let test_files = files
        .iter()
        .filter_map(|file| file["filename"].as_str())
        .filter(|name| is_test(name))
        .map(String::from)
        .collect();
    let missing = files
        .iter()
        .filter(|file| file["patch"].as_str().is_none())
        .count();
    let mut warnings = vec![
        "Test changes are captured; application tests were not executed.".into(),
        "Later commits are temporal evidence only, not proof that a comment was addressed.".into(),
    ];
    if missing > 0 {
        warnings.push(format!("{missing} final file(s) have no API text patch (binary/large/deleted); inspect the saved diff and source."));
    }
    let missing_commit_patches = commits
        .iter()
        .flat_map(|commit| commit["files"].as_array().into_iter().flatten())
        .filter(|file| file["patch"].as_str().is_none())
        .count();
    if missing_commit_patches > 0 {
        warnings.push(format!("{missing_commit_patches} commit file version(s) have no API text patch; historical binary/large-file contents are not captured."));
    }
    Ok((
        Snapshot {
            repository: repository.into(),
            number,
            captured_at: Utc::now(),
            pr,
            files,
            review_comments,
            reviews,
            discussion_comments,
            commits,
            test_files,
            warnings,
            diff_sha256: digest(diff.as_bytes()),
        },
        diff,
    ))
}

pub fn collect(api: &impl GitHubApi, options: &CollectOptions) -> Result<PathBuf> {
    validate_repository(&options.repository)?;
    if !(1..=50).contains(&options.count)
        || options.scan_limit < options.count
        || options.scan_limit > 1000
    {
        return Err("count must be 1–50 and scan-limit must be between count and 1000".into());
    }
    if options.output.exists() {
        return Err(
            "output already exists; choose a new directory to preserve the prior batch".into(),
        );
    }
    let root = format!("repos/{}", options.repository);
    let holdout = if let Some(number) = options.holdout {
        Some(api.json(&format!("{root}/pulls/{number}"))?)
    } else {
        None
    };
    let cutoff = match (&holdout, options.before) {
        (Some(pr), None) => {
            Some(DateTime::parse_from_rfc3339(text(pr, "created_at"))?.with_timezone(&Utc))
        }
        (Some(pr), Some(before)) => Some(
            before.min(DateTime::parse_from_rfc3339(text(pr, "created_at"))?.with_timezone(&Utc)),
        ),
        (None, before) => before,
    };
    if let Some(parent) = options
        .output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    private_directory(&options.output)?;
    write_private(&options.output.join(".gitignore"), b"*\n")?;
    let result = run_collection(api, options, &root, cutoff);
    match result {
        Ok((snapshots, skipped, proposals)) => {
            let proposal_bytes = serde_json::to_vec_pretty(&proposals)?;
            let report = preview::report(&options.repository, &snapshots, &proposals);
            let manifest = json!({"format_version":1,"complete":true,"repository":options.repository,
                "requested_count":options.count,"selected_count":snapshots.len(),"captured_at":Utc::now(),
                "before":cutoff,"held_out":holdout.as_ref().map(|pr| json!({"number":pr["number"],"head_sha":pr["head"]["sha"],"created_at":pr["created_at"]})),
                "selected_prs":snapshots.iter().map(|item| json!({"number":item.number,"head_sha":item.pr["head"]["sha"],"diff_sha256":item.diff_sha256})).collect::<Vec<_>>(),
                "skipped":skipped,"proposal_count":proposals.len(),"published":false,"api_version":"2026-03-10",
                "report_sha256":digest(report.as_bytes()),"proposals_sha256":digest(&proposal_bytes)});
            write_private(&options.output.join("proposals.json"), &proposal_bytes)?;
            write_private(&options.output.join("report.md"), report.as_bytes())?;
            // Complete is the final marker, only after every preview artifact exists.
            write_private(
                &options.output.join("manifest.json"),
                &serde_json::to_vec_pretty(&manifest)?,
            )?;
            Ok(options.output.join("report.md"))
        }
        Err(error) => {
            write_private(
                &options.output.join("incomplete.json"),
                &serde_json::to_vec_pretty(
                    &json!({"complete":false,"reason":error.to_string(),"published":false}),
                )?,
            )?;
            Err(error)
        }
    }
}

type Collected = (Vec<Snapshot>, Vec<Value>, Vec<preview::Proposal>);

fn run_collection(
    api: &impl GitHubApi,
    options: &CollectOptions,
    root: &str,
    cutoff: Option<DateTime<Utc>>,
) -> Result<Collected> {
    let mut snapshots = Vec::new();
    let mut skipped = Vec::new();
    let mut proposals = Vec::new();
    let mut scanned = 0;
    let cutoff_query = cutoff
        .map(|date| {
            format!(
                " merged:<{}",
                date.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            )
        })
        .unwrap_or_default();
    let query = format!(
        "repo:{} is:pr is:merged{cutoff_query} sort:updated-desc",
        options.repository
    );
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    'search: for page in 1..=10 {
        let search = api.json(&format!(
            "search/issues?q={encoded}&per_page=100&page={page}"
        ))?;
        if search["incomplete_results"].as_bool() != Some(false) {
            return Err("GitHub search is incomplete; no successful batch will be emitted".into());
        }
        let items = search["items"]
            .as_array()
            .ok_or("GitHub search returned no items")?;
        if items.is_empty() {
            break;
        }
        for item in items {
            if scanned >= options.scan_limit {
                break 'search;
            }
            scanned += 1;
            let number = item["number"]
                .as_u64()
                .ok_or("search item has no PR number")?;
            if options.holdout == Some(number) {
                continue;
            }
            let pr = api.json(&format!("{root}/pulls/{number}"))?;
            let merged = pr["merged_at"]
                .as_str()
                .map(DateTime::parse_from_rfc3339)
                .transpose()?;
            if merged.is_none()
                || cutoff
                    .zip(merged)
                    .is_some_and(|(cutoff, merged)| merged >= cutoff)
            {
                skipped.push(json!({"number":number,"reason":"not merged before cutoff"}));
                continue;
            }
            if count(&pr, "review_comments")? == 0 || count(&pr, "commits")? < 2 {
                skipped.push(json!({"number":number,"reason":"requires inline review discussion and at least two commits"}));
                continue;
            }
            if count(&pr, "changed_files")? > 3000 || count(&pr, "commits")? > 250 {
                skipped.push(json!({"number":number,"reason":"exceeds complete GitHub files/commits API limits"}));
                continue;
            }
            let comments = pages(
                api,
                &format!("{root}/pulls/{number}/comments"),
                Some(count(&pr, "review_comments")?),
            )?;
            if !comments
                .iter()
                .any(|comment| preview::meaningful(comment, cutoff))
            {
                skipped.push(
                    json!({"number":number,"reason":"no substantive review comment before cutoff"}),
                );
                continue;
            }
            eprintln!(
                "Capturing merged PR #{number} ({}/{})",
                snapshots.len() + 1,
                options.count
            );
            let (snapshot, diff) = capture(api, &options.repository, number, pr)?;
            let folder = options.output.join(format!("pr-{number}"));
            private_directory(&folder)?;
            write_private(
                &folder.join("snapshot.json"),
                &serde_json::to_vec_pretty(&snapshot)?,
            )?;
            write_private(&folder.join("diff.patch"), diff.as_bytes())?;
            proposals.extend(preview::propose(&snapshot, cutoff));
            snapshots.push(snapshot);
            if snapshots.len() == options.count {
                return Ok((snapshots, skipped, proposals));
            }
        }
        if items.len() < 100 {
            break 'search;
        }
    }
    Err(format!("found {} eligible PRs while scanning {scanned}; requested {} (partial snapshots retained, not published)", snapshots.len(),options.count).into())
}
