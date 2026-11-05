//! Best-effort operational outbox: metadata only; failure never blocks learning.
use super::{Result, Settings, evidence, remote};
use crate::{
    activity::{LearningRun, RunPhase, RunState},
    collector,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    future::Future,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize)]
struct LocalRun {
    run: LearningRun,
    reported: bool,
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
type RunContext = (
    RunPhase,
    String,
    Option<u64>,
    Option<String>,
    Option<String>,
);
fn context(settings: &Settings, name: &str, input: &Value) -> Result<RunContext> {
    match name {
        "prepare_chat_learning" => {
            let repo = settings.select_repository(input["repository"].as_str())?;
            let thread = input["thread_id"]
                .as_str()
                .ok_or("selected thread required")?;
            if !crate::conversation::valid_thread(thread) {
                return Err("invalid thread".into());
            }
            Ok((RunPhase::Capture, repo, None, None, Some(thread.into())))
        }
        "remember_chat_lessons" | "reconcile_chat_lessons" => {
            let revision = input["revision"].as_str().ok_or("chat revision required")?;
            let repo = input["repository"].as_str().ok_or("repository required")?;
            let (repo, thread) = super::conversations::run_source(settings, repo, revision)?;
            Ok((
                if name == "remember_chat_lessons" {
                    RunPhase::Publication
                } else {
                    RunPhase::Reconciliation
                },
                repo,
                None,
                Some(revision.into()),
                Some(thread),
            ))
        }
        "prepare_pr_learning" => {
            let parsed: super::PrepareInput = serde_json::from_value(input.clone())?;
            let (repo, pr) = evidence::resolve_pr(settings, &parsed.pr)?;
            Ok((RunPhase::Capture, repo, Some(pr), None, None))
        }
        "remember_pr_lessons" | "reconcile_pr_lessons" => {
            let revision = if name == "remember_pr_lessons" {
                serde_json::from_value::<super::RememberInput>(input.clone())?.revision
            } else {
                serde_json::from_value::<super::reconciliation::ReconcileInput>(input.clone())?
                    .revision
            };
            let (repo, pr) = evidence::run_source(settings, &revision)?;
            Ok((
                if name == "remember_pr_lessons" {
                    RunPhase::Publication
                } else {
                    RunPhase::Reconciliation
                },
                repo,
                Some(pr),
                Some(revision),
                None,
            ))
        }
        _ => Err("operation does not emit learning activity".into()),
    }
}
fn entries(settings: &Settings) -> Vec<(PathBuf, LocalRun)> {
    let Ok(items) = fs::read_dir(settings.cache_dir.join("runs")) else {
        return Vec::new();
    };
    let mut result: Vec<_> = items
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_stem()?.to_str()?;
            if name.len() != 64
                || !name.bytes().all(|c| c.is_ascii_hexdigit())
                || path.extension()?.to_str()? != "json"
                || path.symlink_metadata().ok()?.file_type().is_symlink()
            {
                return None;
            }
            let metadata = path.metadata().ok()?;
            if metadata.len() > 8192 {
                return None;
            }
            let local: LocalRun = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
            local.run.validate().ok()?;
            if local.run.id != name {
                return None;
            }
            settings
                .select_repository(Some(&local.run.repository))
                .ok()?;
            Some((path, local))
        })
        .collect();
    result.sort_by(|a, b| {
        a.1.run
            .client_started_at
            .cmp(&b.1.run.client_started_at)
            .then_with(|| a.1.run.id.cmp(&b.1.run.id))
    });
    result
}
// A short OS lock protects local read/modify/write, never held over network I/O.
fn lock(settings: &Settings) -> Result<fs::File> {
    settings.cache()?;
    let directory = settings.cache_dir.join("runs");
    let metadata = directory.symlink_metadata()?;
    if !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != fs::metadata(&settings.writer_token_file)?.uid()
    {
        return Err("outbox must be a private directory".into());
    }
    let path = directory.join(".lock");
    if path.symlink_metadata().is_ok_and(|m| !m.is_file()) {
        return Err("invalid outbox lock".into());
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    file.try_lock()?;
    Ok(file)
}
fn acknowledge(settings: &Settings, path: &std::path::Path, sent: &LocalRun) -> Result<()> {
    let _lock = lock(settings)?;
    // A completion may have replaced the started record while the request ran.
    let mut current: LocalRun = serde_json::from_slice(&fs::read(path)?)?;
    if current.run == sent.run {
        current.reported = true;
        evidence::atomic_json(path, &current)?;
    }
    Ok(())
}
fn begin(settings: &Settings, name: &str, input: &Value) -> Result<(PathBuf, LocalRun)> {
    let (phase, repository, pr, revision, conversation) = context(settings, name, input)?;
    settings.cache()?;
    let directory = settings.cache_dir.join("runs");
    if !directory.exists() {
        collector::private_directory(&directory).map_err(|_| "could not create run outbox")?;
    }
    let _lock = lock(settings)?;
    let id = collector::digest(
        format!(
            "{}:{}:{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            name
        )
        .as_bytes(),
    );
    let run = LearningRun {
        repository,
        id: id.clone(),
        phase,
        state: RunState::Started,
        client_started_at: now(),
        client_finished_at: None,
        pr,
        conversation,
        revision,
        inserted: None,
        reused: None,
        changed: None,
    };
    let local = LocalRun {
        run,
        reported: false,
    };
    let path = directory.join(format!("{id}.json"));
    evidence::atomic_json(&path, &local)?;
    let records = entries(settings);
    let excess = records.len().saturating_sub(200);
    for (old, _) in records.into_iter().take(excess) {
        let _ = fs::remove_file(old);
    }
    Ok((path, local))
}
pub async fn flush(settings: &Settings) -> &'static str {
    let pending: Vec<_> = entries(settings)
        .into_iter()
        .filter(|(_, local)| !local.reported)
        .collect();
    if pending.is_empty() {
        return "reported";
    }
    let result = tokio::time::timeout(Duration::from_secs(3), async {
        let client = remote::connect(settings).await?;
        let result: Result<()> = async {
            if !remote::supports(&client, "record_learning_run").await? {
                return Err("Pi activity API is unavailable".into());
            }
            // Latest result first, then drain a small older backlog.
            let mut pending = pending;
            let latest = pending.pop().unwrap();
            let batch = std::iter::once(latest).chain(pending.into_iter().take(4));
            for (path, local) in batch {
                let receipt = remote::call(
                    &client,
                    "record_learning_run",
                    serde_json::to_value(&local.run)?,
                )
                .await?;
                if receipt["id"] != local.run.id
                    || receipt["repository"] != local.run.repository
                    || !matches!(receipt["outcome"].as_str(), Some("recorded" | "unchanged"))
                {
                    return Err("unexpected activity receipt".into());
                }
                acknowledge(settings, &path, &local)?;
            }
            Ok(())
        }
        .await;
        let _ = client.cancel().await;
        result
    })
    .await;
    if matches!(result, Ok(Ok(()))) && entries(settings).iter().all(|(_, local)| local.reported) {
        "reported"
    } else {
        "queued"
    }
}
pub fn local_summary(settings: &Settings) -> Value {
    let entries = entries(settings);
    json!({"retained":entries.len(),"queued":entries.iter().filter(|(_,local)|!local.reported).count(),"incomplete":entries.iter().filter(|(_,local)|local.run.state==RunState::Started).count(),"limit":200})
}
pub async fn track<F>(settings: &Settings, name: &str, input: &Value, operation: F) -> Result<Value>
where
    F: Future<Output = Result<Value>>,
{
    if !matches!(
        name,
        "prepare_pr_learning"
            | "remember_pr_lessons"
            | "reconcile_pr_lessons"
            | "prepare_chat_learning"
            | "remember_chat_lessons"
            | "reconcile_chat_lessons"
    ) {
        return operation.await;
    }
    let journal = begin(settings, name, input).ok();
    let mut result = operation.await;
    let mut saved = false;
    if let Some((path, mut local)) = journal {
        local.run.client_finished_at = Some(now());
        local.run.state = match &result {
            Ok(value) if value["already_processed"] == true => RunState::Skipped,
            Ok(_) => RunState::Succeeded,
            Err(_) => RunState::Failed,
        };
        if let Ok(value) = &result {
            if let Some(revision) = value["revision"].as_str() {
                local.run.revision = Some(revision.into());
            }
            if local.run.phase != RunPhase::Capture {
                let items = value["outcomes"].as_array();
                local.run.inserted = Some(items.map_or(0, |items| {
                    items
                        .iter()
                        .filter(|item| item["outcome"] == "inserted")
                        .count()
                }));
                local.run.reused = Some(items.map_or(0, |items| {
                    items
                        .iter()
                        .filter(|item| {
                            matches!(
                                item["outcome"].as_str(),
                                Some("unchanged" | "already_known" | "already_applied")
                            )
                        })
                        .count()
                }));
                local.run.changed = Some(items.map_or(0, |items| {
                    items
                        .iter()
                        .filter(|item| item["outcome"] == "applied")
                        .count()
                }));
            }
        }
        saved = local.run.validate().is_ok()
            && lock(settings).is_ok_and(|_lock| evidence::atomic_json(&path, &local).is_ok());
    }
    let reporting = if saved {
        flush(settings).await
    } else {
        "unavailable"
    };
    if let Ok(value) = &mut result
        && let Some(object) = value.as_object_mut()
    {
        object.insert("activity_reporting".into(), json!(reporting));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outbox_is_bounded_and_a_stale_ack_cannot_replace_completion() {
        let dir = tempfile::tempdir().unwrap();
        let writer = dir.path().join("writer.env");
        collector::write_private(&writer, b"unused").unwrap();
        let settings = Settings {
            repository: "example/project".into(),
            repositories: vec![],
            endpoint: "http://127.0.0.1:1/mcp".into(),
            writer_token_file: writer,
            learn_conversations: false,
            cache_dir: dir.path().join("cache"),
        };
        let input = json!({"pr":"https://github.com/example/project/pull/12"});
        let (path, local) = begin(&settings, "prepare_pr_learning", &input).unwrap();
        let mut finished = LocalRun {
            run: local.run.clone(),
            reported: false,
        };
        finished.run.state = RunState::Failed;
        finished.run.client_finished_at = Some(now());
        {
            let _lock = lock(&settings).unwrap();
            evidence::atomic_json(&path, &finished).unwrap();
        }
        acknowledge(&settings, &path, &local).unwrap();
        let current: LocalRun = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(current.run.state, RunState::Failed);
        assert!(!current.reported);
        for _ in 0..202 {
            begin(&settings, "prepare_pr_learning", &input).unwrap();
        }
        assert_eq!(entries(&settings).len(), 200);
    }
}
