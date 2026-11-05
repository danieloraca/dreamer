//! Agent bridge for selected Codex thread views. No history crawler or model API.
use super::{Result, Settings, evidence::atomic_json, remote};
use crate::{
    collector,
    conversation::{ConversationClaim, ConversationEvidence, ConversationKind},
    lesson::{ExtractionMethod, Lesson, LessonStatus, Provenance},
    retrieval::LessonScope,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
mod chunks;
mod reader;
pub use chunks::{ChatBundleInput, bundle, stage};
pub use reader::{CodexChunkInput, prepare_codex};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub item_id: String,
    pub text: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareChatInput {
    pub repository: String,
    pub thread_id: String,
    /// Decoded Codex read_thread response, not an MCP content wrapper. Omit to read the saved progress hint.
    #[serde(default)]
    pub page: Option<Value>,
    /// Sanitized interpretation of an inspected, complete tool result. Not its raw log.
    #[serde(default)]
    pub observations: Vec<Observation>,
    /// Additional customer/resource strings to redact before retention. Never retained.
    #[serde(default)]
    pub sensitive_terms: Vec<String>,
    /// Opt into complete message fragments. Offset counts fragments, not source items.
    /// Omit to preserve the legacy bounded-excerpt format.
    #[serde(default)]
    pub fragment_offset: Option<usize>,
}
#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatReadInput {
    pub repository: String,
    pub revision: String,
    #[serde(default)]
    pub ids: Vec<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatDraft {
    pub summary: String,
    pub applies_when: String,
    pub claim: ConversationClaim,
    #[serde(default)]
    pub scope: LessonScope,
    pub evidence_ids: Vec<String>,
    /// Required for a new lesson: a narrow stable claim/context key, never a broad component or date. Omit only for an exact legacy retry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_key: Option<String>,
    #[serde(default)]
    pub assessment_token: Option<String>,
    #[serde(default)]
    pub related_assessments: Vec<RelatedAssessment>,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelatedAssessment {
    pub id: String,
    pub relation: RelatedRelation,
    pub reason: String,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelatedRelation {
    Consistent,
    Historical,
    Conflict,
    Correction,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RememberChatInput {
    pub repository: String,
    pub revision: String,
    #[serde(default)]
    pub lessons: Vec<ChatDraft>,
    #[serde(default)]
    pub skip_reason: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Item {
    id: String,
    turn_id: String,
    kind: ConversationKind,
    text: String,
    content_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_time: Option<crate::chronology::SourceTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fragment: Option<chunks::Fragment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    item_started_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    item_completed_at_ms: Option<i64>,
}
impl Item {
    fn source_id(&self) -> &str {
        self.source_item_id.as_deref().unwrap_or(&self.id)
    }
}
#[derive(Serialize, Deserialize)]
struct Snapshot {
    repository: String,
    thread_id: String,
    observed_at: String,
    items: Vec<Item>,
    next_cursor: Option<String>,
    has_more: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    next_fragment_offset: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    parents: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    item_pagination: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    fragmented: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    resolutions: Vec<chunks::ProposalResolution>,
}
fn fingerprint(s: &Snapshot) -> String {
    let mut identity = json!([s.repository, s.thread_id, s.items]);
    if s.item_pagination || s.fragmented || !s.parents.is_empty() {
        identity.as_array_mut().unwrap().push(json!([
            s.item_pagination,
            s.parents,
            s.resolutions,
            s.fragmented,
            s.next_fragment_offset
        ]));
    }
    collector::digest(&serde_json::to_vec(&identity).unwrap())
}
fn receipt_complete(dir: &Path, name: &str) -> Result<bool> {
    Ok(optional_json::<Value>(&dir.join(name))?.is_some_and(|v| v["complete"] == true))
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn private(path: &Path, is_dir: bool) -> Result<()> {
    let m = path.symlink_metadata()?;
    if (if is_dir { !m.is_dir() } else { !m.is_file() }) || m.permissions().mode() & 0o077 != 0 {
        return Err("chat cache must contain private regular files/directories".into());
    }
    Ok(())
}
fn root(settings: &Settings) -> Result<PathBuf> {
    if !settings.learn_conversations {
        return Err("conversation learning is disabled; enable learn_conversations in Mac settings for selected chats".into());
    }
    settings.cache()?;
    let path = settings.cache_dir.join("chats");
    if !path.exists() {
        collector::private_directory(&path).map_err(|_| "could not create private chat cache")?;
    }
    private(&path, true)?;
    if path.metadata()?.uid() != settings.cache_dir.metadata()?.uid() {
        return Err("chat cache owner mismatch".into());
    }
    Ok(path)
}
fn lock(path: &Path) -> Result<fs::File> {
    if path.symlink_metadata().is_ok() {
        private(path, false)?;
    }
    let f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    f.try_lock()?;
    Ok(f)
}
fn folder(settings: &Settings, revision: &str) -> Result<PathBuf> {
    if !crate::conversation::digest(revision) {
        return Err("invalid chat revision".into());
    }
    let p = root(settings)?.join(revision);
    private(&p, true)?;
    Ok(p)
}
fn read_json<T: serde::de::DeserializeOwned>(p: &Path) -> Result<T> {
    private(p, false)?;
    if p.metadata()?.len() > 512_000 {
        return Err("oversized chat cache record".into());
    }
    Ok(serde_json::from_slice(&fs::read(p)?)?)
}
fn optional_json<T: serde::de::DeserializeOwned>(p: &Path) -> Result<Option<T>> {
    match p.symlink_metadata() {
        Ok(_) => Ok(Some(read_json(p)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn load(settings: &Settings, repo: &str, revision: &str) -> Result<Snapshot> {
    settings.select_repository(Some(repo))?;
    let s: Snapshot = read_json(&folder(settings, revision)?.join("snapshot.json"))?;
    if s.repository != repo || fingerprint(&s) != revision {
        return Err("chat source scope or revision mismatch".into());
    }
    Ok(s)
}
/// Conservative automatic removal, supplemented by caller-provided sensitive terms.
/// This does not recognize every name or business identifier; callers must minimize text.
pub fn sanitise(text: &str, sensitive: &[String]) -> String {
    let mut text = text.to_owned();
    for term in sensitive.iter().filter(|s| !s.trim().is_empty()) {
        let mut replaced = String::new();
        let lower = text.to_ascii_lowercase();
        let mut cursor = 0;
        while let Some(offset) = lower[cursor..].find(&term.to_ascii_lowercase()) {
            let at = cursor + offset;
            replaced.push_str(&text[cursor..at]);
            replaced.push_str("[redacted]");
            cursor = at + term.len();
        }
        replaced.push_str(&text[cursor..]);
        text = replaced;
    }
    text.lines()
        .map(|line| {
            let lower = line
                .to_ascii_lowercase()
                .replace([' ', '\t', '"', '\'', '`'], "");
            if [
                "authorization:",
                "password=",
                "password:",
                "secret=",
                "secret:",
                "token=",
                "token:",
                "api_key:",
                "api_key=",
                "privatekey",
                "bearer",
            ]
            .iter()
            .any(|s| lower.contains(s))
            {
                return "[redacted sensitive line]".into();
            }
            line.split_whitespace()
                .map(|word| {
                    let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
                    if word.contains('@')
                        || word.contains("://")
                        || clean.starts_with("ghp_")
                        || clean.starts_with("sk-")
                        || (clean.len() >= 26
                            && clean
                                .bytes()
                                .all(|c| c.is_ascii_alphanumeric() || c == b'-'))
                    {
                        "[redacted]".into()
                    } else {
                        word.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<String>>()
        .join("\n")
}
fn safe_text(text: &str) -> bool {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
        == sanitise(text, &[])
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
}
fn short(text: &str, bytes: usize) -> String {
    let mut n = text.len().min(bytes);
    while !text.is_char_boundary(n) {
        n -= 1;
    }
    text[..n].to_owned()
}
fn page_items(input: &PrepareChatInput) -> Result<Snapshot> {
    let p = input
        .page
        .as_ref()
        .ok_or("supply a selected Codex thread page")?;
    if serde_json::to_vec(p)?.len() > 512_000
        || input.sensitive_terms.len() > 50
        || input
            .sensitive_terms
            .iter()
            .any(|s| s.len() > 200 || s.contains("[redacted]"))
        || input.observations.len() > 20
    {
        return Err("chat input exceeds bounds".into());
    }
    if p["thread"]["id"] != input.thread_id
        || p["thread"]["kind"] != "codex"
        || !crate::conversation::valid_thread(&input.thread_id)
    {
        return Err("selected thread identity/provider mismatch".into());
    }
    let turns = p["turns"].as_array().ok_or("missing Codex turns")?;
    if turns.len() > 20 {
        return Err("capture at most 20 completed turns per page".into());
    }
    let mut items = Vec::new();
    let mut ids = BTreeSet::new();
    let mut observations: BTreeMap<_, _> = input
        .observations
        .iter()
        .map(|o| (o.item_id.clone(), o.text.clone()))
        .collect();
    if observations.len() != input.observations.len() {
        return Err("duplicate observed item".into());
    }
    for t in turns {
        if t["status"] != "completed" {
            continue;
        }
        let turn = t["id"].as_str().ok_or("turn needs identity")?;
        if !crate::conversation::valid_id(turn) {
            return Err("invalid turn identity".into());
        }
        for i in t["items"].as_array().ok_or("turn needs items")? {
            let typ = i["type"].as_str().unwrap_or("");
            let mut result_digest = None;
            // Do not serialize or retain reasoning, arguments, command text or raw logs.
            let (kind, text) = match typ {
                "userMessage" => (
                    ConversationKind::UserStatement,
                    i["content"]
                        .as_array()
                        .ok_or("user message needs text content")?
                        .iter()
                        .filter(|c| c["type"] == "text" || c["type"] == "input_text")
                        .filter_map(|c| c["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                "agentMessage" if i["phase"] == "final_answer" => (
                    ConversationKind::AssistantReport,
                    i["text"].as_str().unwrap_or("").to_owned(),
                ),
                "mcpToolCall" | "commandExecution" => {
                    let id = i["id"].as_str().unwrap_or("");
                    if let Some(text) = observations.remove(id) {
                        let output = i
                            .get("result")
                            .or(i.get("output"))
                            .or(i.get("aggregatedOutput"));
                        if !matches!(i["status"].as_str(), Some("completed" | "failed"))
                            || output.is_none_or(Value::is_null)
                            || [
                                "truncated",
                                "outputTruncated",
                                "aggregatedOutputTruncated",
                                "resultTruncated",
                            ]
                            .iter()
                            .any(|k| i[*k] == true)
                        {
                            return Err("observed result needs an inspected complete output, not a success status or truncated view".into());
                        }
                        if !(15..=1200).contains(&text.len()) {
                            return Err(
                                "observations need 15–1200 bytes of minimized interpretation"
                                    .into(),
                            );
                        }
                        result_digest =
                            Some(collector::digest(&serde_json::to_vec(output.unwrap())?));
                        (ConversationKind::ObservedResult, text)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };
            if text.trim().is_empty() {
                continue;
            }
            let id = i["id"].as_str().ok_or("source item needs identity")?;
            if !crate::conversation::valid_id(id) || !ids.insert(id.to_owned()) {
                return Err("duplicate or invalid chat item identity".into());
            }
            let sanitized = sanitise(&text, &input.sensitive_terms);
            let content_sha256 = match result_digest {
                Some(result) => {
                    collector::digest(&serde_json::to_vec(&json!([sanitized, result]))?)
                }
                None => collector::digest(sanitized.as_bytes()),
            };
            let source_time = capture_time(i, t, turn)?;
            let item = Item {
                source_time,
                id: id.into(),
                turn_id: turn.into(),
                kind,
                content_sha256,
                text: short(&sanitized, 1600),
                source_item_id: None,
                fragment: None,
                item_started_at_ms: i["_source_started_at_ms"].as_i64(),
                item_completed_at_ms: i["_source_completed_at_ms"].as_i64(),
            };
            if input.fragment_offset.is_some() {
                items.extend(chunks::fragments(item, &sanitized));
            } else {
                items.push(item);
            }
            if input.fragment_offset.is_none() && items.len() > 60 {
                return Err("capture at most 60 useful message/result excerpts".into());
            }
        }
    }
    if !observations.is_empty() {
        return Err("observation must cite a result item in a completed turn".into());
    }
    if items.is_empty() {
        return Err("page has no usable completed user/final messages or result evidence".into());
    }
    let next_fragment_offset = if let Some(offset) = input.fragment_offset {
        if offset >= items.len() {
            return Err("fragment offset is outside this source view".into());
        }
        let end = (offset + 8).min(items.len());
        let next = (end < items.len()).then_some(end);
        items = items[offset..end].to_vec();
        next
    } else {
        None
    };
    items.sort_by(|a, b| a.id.cmp(&b.id));
    let cursor = p["page"]["nextCursor"].as_str().map(str::to_owned);
    if cursor.as_ref().is_some_and(|s| s.len() > 4096) {
        return Err("oversized page cursor".into());
    }
    Ok(Snapshot {
        repository: input.repository.clone(),
        thread_id: input.thread_id.clone(),
        observed_at: now(),
        items,
        next_cursor: cursor,
        has_more: p["page"]["hasMore"] == true,
        next_fragment_offset,
        parents: vec![],
        item_pagination: p["page"]["adapter"] == "codex_items",
        fragmented: input.fragment_offset.is_some(),
        resolutions: vec![],
    })
}
fn thread_key(s: &Snapshot) -> String {
    collector::digest(format!("{}:{}", s.repository, s.thread_id).as_bytes())
}
fn current(settings: &Settings, s: &Snapshot) -> Result<()> {
    let index: BTreeMap<String, String> =
        read_json(&root(settings)?.join(format!("{}.items.json", thread_key(s))))?;
    if s.items
        .iter()
        .any(|i| index.get(i.source_id()) != Some(&i.content_sha256))
    {
        return Err(
            "known chat item changed; prepare the revised view and reassess old lessons".into(),
        );
    }
    Ok(())
}
fn prune(path: &Path) -> Result<()> {
    let mut dirs = Vec::new();
    for e in fs::read_dir(path)? {
        let e = e?;
        let name = e.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !crate::conversation::digest(name) {
            continue;
        }
        private(&e.path(), true)?;
        let s: Snapshot = read_json(&e.path().join("snapshot.json"))?;
        let done = !pending_reconciliation(&e.path())?
            && ["publication.json", "stage-consumed.json"]
                .iter()
                .any(|name| {
                    read_json::<Value>(&e.path().join(name)).is_ok_and(|v| v["complete"] == true)
                });
        dirs.push((s.observed_at, e.path(), done));
    }
    dirs.sort();
    let mut count = dirs.len();
    let cutoff = chrono::Utc::now() - chrono::Duration::days(30);
    for (at, dir, done) in dirs {
        if done
            && (count >= 128
                || chrono::DateTime::parse_from_rfc3339(&at)?.with_timezone(&chrono::Utc) < cutoff)
        {
            let Ok(_publication) = lock(&dir.join(".publication.lock")) else {
                continue;
            };
            fs::remove_dir_all(dir)?;
            count -= 1;
        }
    }
    let mut active = BTreeSet::new();
    for e in fs::read_dir(path)? {
        let e = e?;
        if e.file_name()
            .to_str()
            .is_some_and(crate::conversation::digest)
        {
            let s: Snapshot = read_json(&e.path().join("snapshot.json"))?;
            active.insert(thread_key(&s));
        }
    }
    for e in fs::read_dir(path)? {
        let e = e?;
        let n = e.file_name();
        let Some(n) = n.to_str() else {
            continue;
        };
        if let Some(key) = n
            .strip_suffix(".items.json")
            .or(n.strip_suffix(".progress.json"))
            && crate::conversation::digest(key)
            && !active.contains(key)
        {
            private(&e.path(), false)?;
            fs::remove_file(e.path())?;
        }
    }
    if count >= 128 {
        return Err("chat cache full of unfinished revisions; finish pending publications before capturing more".into());
    }
    Ok(())
}
pub fn prepare(settings: &Settings, input: PrepareChatInput) -> Result<Value> {
    settings.select_repository(Some(&input.repository))?;
    let r = root(settings)?;
    if !crate::conversation::valid_thread(&input.thread_id) {
        return Err("invalid selected thread".into());
    }
    if input.page.is_none() {
        let key = collector::digest(format!("{}:{}", input.repository, input.thread_id).as_bytes());
        let progress = optional_json::<Value>(&r.join(format!("{key}.progress.json")))?;
        return Ok(
            json!({"repository":input.repository,"thread_id":input.thread_id,"progress":progress,"guidance":"Read only this selected chat with Codex read_thread. A saved cursor is a progress hint; revisit edited turns. No full-history completion is claimed."}),
        );
    }
    capture(settings, page_items(&input)?)
}
fn capture(settings: &Settings, mut s: Snapshot) -> Result<Value> {
    let r = root(settings)?;
    let _guard = lock(&r.join(".lock"))?;
    if !s.parents.is_empty() {
        current(settings, &s)?;
    }
    let revision = fingerprint(&s);
    let dir = r.join(&revision);
    if !dir.exists() {
        prune(&r)?;
        collector::private_directory(&dir).map_err(|_| "could not create private chat revision")?;
        atomic_json(&dir.join("snapshot.json"), &s)?;
    } else {
        s = load(settings, &s.repository, &revision)?;
    }
    let index_path = r.join(format!("{}.items.json", thread_key(&s)));
    let mut index: BTreeMap<String, String> = optional_json(&index_path)?.unwrap_or_default();
    let changed: Vec<_> = s
        .items
        .iter()
        .filter(|i| {
            index
                .get(i.source_id())
                .is_some_and(|old| old != &i.content_sha256)
        })
        .map(|i| i.source_id().to_owned())
        .collect();
    for i in &s.items {
        index.insert(i.source_id().to_owned(), i.content_sha256.clone());
    }
    if index.len() > 2048 {
        return Err(
            "selected chat identity index exceeds 2048 items; use bounded task excerpts".into(),
        );
    }
    atomic_json(&index_path, &index)?;
    let pending = pending_reconciliation(&dir)?;
    let done = !pending
        && (receipt_complete(&dir, "publication.json")?
            || receipt_complete(&dir, "stage-consumed.json")?);
    Ok(
        json!({"repository":s.repository,"thread_id":s.thread_id,"revision":revision,"already_processed":done,"pending_reconciliation":pending,"changed_item_ids":changed,"evidence":s.items.iter().map(|i|json!({"id":i.id,"source_item_id":i.source_id(),"fragment":i.fragment,"turn_id":i.turn_id,"kind":i.kind,"bytes":i.text.len()})).collect::<Vec<_>>(),"next_fragment_offset":s.next_fragment_offset,"next_cursor":s.next_cursor,"has_more":s.has_more,"coverage":"bounded completed-turn excerpts, not whole-chat completion","guidance":"Read useful source IDs; distinguish user decisions, inspected results and assistant reports. Stage chunks, consolidate equivalent claims with existing knowledge, then bundle selected evidence before assessment/publication. No per-lesson approval. Reasoning and raw arguments/commands are excluded. Inspect current docs/code before technical claims."}),
    )
}
pub fn read(settings: &Settings, input: ChatReadInput) -> Result<Value> {
    let s = load(settings, &input.repository, &input.revision)?;
    current(settings, &s)?;
    if input.ids.is_empty() || input.ids.len() > 8 {
        return Err("read 1–8 chat source IDs".into());
    }
    let mut selected = Vec::new();
    for id in &input.ids {
        selected.push(
            s.items
                .iter()
                .find(|i| &i.id == id)
                .ok_or("unknown chat evidence ID")?
                .clone(),
        );
    }
    let dir = folder(settings, &input.revision)?;
    let _lock = lock(&dir.join(".lock"))?;
    let path = dir.join("read.json");
    let mut inspected: BTreeSet<String> = optional_json(&path)?.unwrap_or_default();
    inspected.extend(input.ids);
    atomic_json(&path, &inspected)?;
    Ok(
        json!({"repository":s.repository,"revision":input.revision,"sources":selected,"staged_proposals":optional_json::<Value>(&dir.join("staged.json"))?,"coverage":"sanitized excerpts; source status/assistant claims alone are not verified business outcomes"}),
    )
}
pub fn run_source(settings: &Settings, repo: &str, revision: &str) -> Result<(String, String)> {
    let source = load(settings, repo, revision)?;
    Ok((source.repository, source.thread_id))
}
pub fn build(
    settings: &Settings,
    repo: &str,
    revision: &str,
    drafts: &[ChatDraft],
) -> Result<Vec<Lesson>> {
    let s = load(settings, repo, revision)?;
    current(settings, &s)?;
    if drafts.len() > 10 {
        return Err("at most ten chat lessons".into());
    }
    let inspected: BTreeSet<String> = read_json(&folder(settings, revision)?.join("read.json"))?;
    let mut lessons = Vec::new();
    let mut ids = BTreeSet::new();
    for d in drafts {
        if !(15..=2000).contains(&d.summary.trim().len())
            || !(15..=3000).contains(&d.applies_when.trim().len())
            || d.evidence_ids.is_empty()
            || d.evidence_ids.len() > 12
            || !safe_text(&d.summary)
            || !safe_text(&d.applies_when)
        {
            return Err("chat drafts need bounded minimized text and 1–12 inspected sources; remove sensitive identifiers".into());
        }
        let mut proof: Vec<ConversationEvidence> = Vec::new();
        let mut seen = BTreeSet::new();
        let mut selected = Vec::new();
        for id in &d.evidence_ids {
            if !inspected.contains(id) || !seen.insert(id) {
                return Err("read every unique cited chat source first".into());
            }
            let i = s
                .items
                .iter()
                .find(|i| &i.id == id)
                .ok_or("missing chat source")?;
            selected.push(i);
        }
        selected.sort_by(|a, b| {
            (
                a.turn_id.as_str(),
                a.source_id(),
                a.fragment.as_ref().map(|f| (f.start, f.end)),
            )
                .cmp(&(
                    b.turn_id.as_str(),
                    b.source_id(),
                    b.fragment.as_ref().map(|f| (f.start, f.end)),
                ))
        });
        for i in selected {
            if let Some(existing) = proof
                .iter_mut()
                .find(|e| e.item_id == i.source_id() && e.turn_id == i.turn_id)
            {
                if existing.content_sha256 != i.content_sha256 || existing.kind != i.kind {
                    return Err("mixed message versions in lesson".into());
                }
                if let Some(span) = &i.fragment {
                    existing.description.push_str(&format!(
                        " Also inspected bytes {}..{}.",
                        span.start, span.end
                    ));
                }
            } else {
                proof.push(ConversationEvidence {
                    source_time: i.source_time.clone(),
                    provider: "codex".into(),
                    thread_id: s.thread_id.clone(),
                    turn_id: i.turn_id.clone(),
                    item_id: i.source_id().into(),
                    source_url: format!("codex://threads/{}", s.thread_id),
                    observed_at: s.observed_at.clone(),
                    content_sha256: i.content_sha256.clone(),
                    kind: i.kind,
                    claim: d.claim,
                    description: chunks::description(i),
                });
            }
        }
        proof.sort_by(|a, b| a.item_id.cmp(&b.item_id));
        let mut identity = serde_json::to_value(&proof)?;
        for item in identity.as_array_mut().unwrap() {
            item.as_object_mut().unwrap().remove("observed_at");
        }
        let scope = d.scope.canonical();
        let mut identity_data = json!([
            repo,
            d.summary.split_whitespace().collect::<Vec<_>>().join(" "),
            d.applies_when
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            scope,
            identity
        ]);
        if let Some(key) = &d.knowledge_key {
            identity_data.as_array_mut().unwrap().push(json!(key));
        }
        let id = format!(
            "chat-{}",
            &collector::digest(&serde_json::to_vec(&identity_data)?)[..24]
        );
        let l = Lesson {
            id,
            repository: repo.into(),
            summary: d.summary.trim().into(),
            applies_when: d.applies_when.trim().into(),
            status: LessonStatus::Candidate,
            scope,
            knowledge_key: d.knowledge_key.clone(),
            provenance: Provenance {
                producer:
                    "Codex agent via Dreamer Mac; conversation-derived, not independently verified"
                        .into(),
                method: ExtractionMethod::Manual,
            },
            evidence: vec![],
            conversation_evidence: proof,
        };
        l.validate_candidate()?;
        if !ids.insert(l.id.clone()) {
            return Err("duplicate chat lesson draft".into());
        }
        lessons.push(l);
    }
    Ok(lessons)
}
fn finish(settings: &Settings, s: &Snapshot, revision: &str, value: &Value) -> Result<()> {
    let r = root(settings)?;
    let _lock = lock(&r.join(".lock"))?;
    current(settings, s)?;
    atomic_json(&folder(settings, revision)?.join("publication.json"), value)?;
    for parent in &s.parents {
        if !r.join(parent).try_exists()? {
            continue;
        }
        atomic_json(
            &folder(settings, parent)?.join("stage-consumed.json"),
            &json!({"complete":true,"bundle_revision":revision,"resolutions":s.resolutions.iter().filter(|r|&r.revision==parent).collect::<Vec<_>>()}),
        )?;
    }
    if s.item_pagination || !s.parents.is_empty() {
        return Ok(());
    }
    atomic_json(
        &r.join(format!("{}.progress.json", thread_key(s))),
        &json!({"repository":s.repository,"thread_id":s.thread_id,"revision":revision,"next_cursor":s.next_cursor,"has_more":s.has_more,"completed_at":now(),"coverage":"this captured page only"}),
    )
}
pub async fn remember(settings: &Settings, input: RememberChatInput) -> Result<Value> {
    let s = load(settings, &input.repository, &input.revision)?;
    let dir = folder(settings, &input.revision)?;
    let _lock = lock(&dir.join(".publication.lock"))?;
    if pending_reconciliation(&dir)? {
        return Err("finish pending chat corrections before completing this page".into());
    }
    let drafts = serde_json::to_value(&input.lessons)?;
    let journal = dir.join("publication.json");
    let prior = optional_json::<Value>(&journal)?;
    if prior.as_ref().is_some_and(|v| {
        v["complete"] == false && draft_identity(&v["drafts"]) != draft_identity(&drafts)
    }) {
        return Err("resume the same pending chat drafts before changing the batch".into());
    }
    current(settings, &s)?;
    let proposed: BTreeSet<_> = input
        .lessons
        .iter()
        .filter_map(|d| d.knowledge_key.as_deref())
        .collect();
    if s.resolutions.iter().any(|r| {
        r.action == chunks::ResolutionAction::Include
            && !proposed.contains(r.knowledge_key.as_str())
    }) {
        return Err("consolidated publication must address every included staged claim; revise bundle dispositions explicitly before publication".into());
    }
    let lessons = build(settings, &input.repository, &input.revision, &input.lessons)?;
    if lessons.is_empty() {
        let reason = input
            .skip_reason
            .as_deref()
            .ok_or("empty chat lessons need a specific skip_reason")?;
        if !(15..=1000).contains(&reason.len()) || !safe_text(reason) {
            return Err("use a bounded sanitized skip reason".into());
        }
        let v = json!({"complete":true,"repository":s.repository,"revision":input.revision,"outcomes":[],"skip_reason":reason,"manual_approval_required":false});
        finish(settings, &s, &input.revision, &v)?;
        return Ok(v);
    }
    if input.skip_reason.is_some() {
        return Err("skip_reason applies only to an empty batch".into());
    }
    let client = remote::connect(settings).await?;
    let result=async {
        if !remote::supports(&client,"assess_conversation_candidate").await?{return Err("upgrade the Pi to schema 7 for chronology-safe chat learning".into());}
        // Inspect the complete batch before journalling any possible write.
        let mut preflights=Vec::new();
        for (l,draft) in lessons.iter().zip(&input.lessons) {
            current(settings,&s)?;
            let old=remote::call(&client,"get_lesson",json!({"repository":s.repository,"id":l.id})).await?;
            if old["found"]==true {
                let mut existing:Lesson=serde_json::from_value(old["lesson"]["lesson"].clone())?;
                for (a,b) in existing.conversation_evidence.iter_mut().zip(&l.conversation_evidence){a.observed_at=b.observed_at.clone();}
                if existing!=*l{return Err("chat lesson identity conflict; inspect stored provenance".into());}
                preflights.push(None);
            } else {
                let assessment=remote::call(&client,"assess_conversation_candidate",json!({"lesson":l})).await?;
                validate_assessments(l,draft,&assessment)?;
                preflights.push(Some(assessment["assessment_token"].clone()));
            }
        }
        let mut outcomes=Vec::new();
        for (l,token) in lessons.iter().zip(preflights) {
            current(settings,&s)?;
            let outcome=if let Some(token)=token {
                atomic_json(&journal,&json!({"complete":false,"revision":input.revision,"drafts":drafts,"outcomes":outcomes,"in_flight":l.id}))?;
                let receipt=remote::call(&client,"submit_conversation_candidate",json!({"lesson":l,"assessment_token":token})).await?;
                if receipt["id"]!=l.id||receipt["repository"]!=s.repository||!matches!(receipt["outcome"].as_str(),Some("inserted"|"unchanged")){return Err("unexpected chat publication receipt; retry saved drafts".into());}
                let got=remote::call(&client,"get_lesson",json!({"repository":s.repository,"id":l.id})).await?;
                if got["found"]!=true||got["lesson"]["lesson"]!=serde_json::to_value(l)?{return Err("chat publication readback mismatch; retry saved drafts".into());}receipt
            }else{json!({"id":l.id,"repository":s.repository,"outcome":"already_known"})};
            outcomes.push(outcome);
        }
        let v=json!({"complete":true,"repository":s.repository,"revision":input.revision,"outcomes":outcomes,"drafts":drafts,"manual_approval_required":false});finish(settings,&s,&input.revision,&v)?;Ok(v)
    }.await;
    let _ = client.cancel().await;
    result
}

#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatChangeDraft {
    pub source_id: String,
    pub action: crate::lifecycle::ChangeAction,
    #[serde(default)]
    pub target_id: Option<String>,
    pub reason: String,
    pub claim: ConversationClaim,
    pub evidence_ids: Vec<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReconcileChatInput {
    pub repository: String,
    pub revision: String,
    #[serde(default)]
    pub changes: Vec<ChatChangeDraft>,
}
fn pending_reconciliation(dir: &Path) -> Result<bool> {
    Ok(optional_json::<Value>(&dir.join("reconciliation.json"))?
        .is_some_and(|v| v["complete"] == false))
}
pub async fn reconcile(settings: &Settings, mut input: ReconcileChatInput) -> Result<Value> {
    use crate::lifecycle::{ChangeAction, LessonChange};
    let s = load(settings, &input.repository, &input.revision)?;
    current(settings, &s)?;
    let dir = folder(settings, &input.revision)?;
    let _lock = lock(&dir.join(".publication.lock"))?;
    let path = dir.join("reconciliation.json");
    let prior = optional_json::<Value>(&path)?;
    if input.changes.is_empty() {
        input.changes = serde_json::from_value(
            prior
                .as_ref()
                .filter(|v| v["complete"] == false)
                .ok_or("no pending chat corrections")?["drafts"]
                .clone(),
        )?;
    }
    if input.changes.is_empty() || input.changes.len() > 10 {
        return Err("supply 1–10 chat corrections".into());
    }
    let drafts = serde_json::to_value(&input.changes)?;
    if prior
        .as_ref()
        .is_some_and(|v| v["complete"] == false && v["drafts"] != drafts)
    {
        return Err("resume identical pending chat corrections".into());
    }
    let proof_drafts: Vec<_> = input
        .changes
        .iter()
        .map(|d| ChatDraft {
            summary: d.reason.clone(),
            applies_when: "Correct existing knowledge from the inspected conversation evidence."
                .into(),
            claim: d.claim,
            scope: Default::default(),
            evidence_ids: d.evidence_ids.clone(),
            knowledge_key: None,
            assessment_token: None,
            related_assessments: vec![],
        })
        .collect();
    let proofs: Vec<_> = proof_drafts
        .iter()
        .map(|draft| {
            build(
                settings,
                &input.repository,
                &input.revision,
                std::slice::from_ref(draft),
            )
            .map(|mut lessons| lessons.remove(0))
        })
        .collect::<Result<_>>()?;
    let mut changes = Vec::new();
    let mut identities = BTreeSet::new();
    for (draft, proof) in input.changes.iter().zip(proofs) {
        let c=LessonChange{repository:s.repository.clone(),source_id:draft.source_id.clone(),action:draft.action,target_id:draft.target_id.clone(),reason:draft.reason.clone(),producer:"Codex agent via Dreamer Mac; conversation-backed correction, not independently verified".into(),evidence:vec![],conversation_evidence:proof.conversation_evidence}.canonical();
        c.validate()?;
        if !identities.insert(c.id()) {
            return Err("duplicate chat correction".into());
        }
        changes.push(c);
    }
    let client = remote::connect(settings).await?;
    let result=async {
        for name in ["assess_conversation_candidate","assess_lesson_change","change_lesson","get_lesson_history"] {if !remote::supports(&client,name).await?{return Err("upgrade Pi before conversation corrections".into());}}
        let mut completed=BTreeSet::new();let mut retired=BTreeSet::new();
        for c in &changes {
            let history=remote::call(&client,"get_lesson_history",json!({"repository":s.repository,"id":c.source_id,"change_id":c.id(),"limit":1})).await?;
            if let Some(record)=history["changes"].as_array().ok_or("missing correction history")?.first() {
                if record["id"]!=c.id()||record["change"]!=serde_json::to_value(c)?{return Err("chat correction identity conflict".into());}completed.insert(c.id());continue;
            }
            for id in std::iter::once(&c.source_id).chain(c.target_id.iter()) {
                if retired.contains(id){return Err("batch references a retired lesson; select its successor".into());}
                let got=remote::call(&client,"get_lesson",json!({"repository":s.repository,"id":id})).await?;
                if got["found"]!=true||got["lesson"]["lesson"]["repository"]!=s.repository||got["lesson"]["lesson"]["status"]=="deprecated" {return Err("chat correction requires active scoped source and target lessons".into());}
            }
            let checked=remote::call(&client,"assess_lesson_change",json!({"change":c})).await?;
            if checked["allowed"]!=true{return Err("chronological change preflight failed".into());}
            if c.action!=ChangeAction::Conflict{retired.insert(c.source_id.clone());}
        }
        let mut outcomes=Vec::new();
        for c in &changes {
            if completed.contains(&c.id()){outcomes.push(json!({"id":c.id(),"outcome":"already_applied"}));continue;}
            current(settings,&s)?;
            atomic_json(&path,&json!({"complete":false,"revision":input.revision,"drafts":drafts,"outcomes":outcomes,"in_flight":c.id()}))?;
            let receipt=remote::call(&client,"change_lesson",json!({"change":c})).await?;
            if receipt["record"]["id"]!=c.id()||receipt["record"]["change"]!=serde_json::to_value(c)?||!matches!(receipt["outcome"].as_str(),Some("applied"|"unchanged")){return Err("chat correction receipt mismatch".into());}
            let got=remote::call(&client,"get_lesson_history",json!({"repository":s.repository,"id":c.source_id,"change_id":c.id(),"limit":1})).await?;
            if got["changes"][0]!=receipt["record"]{return Err("chat correction readback mismatch; retry identical batch".into());}
            outcomes.push(json!({"id":c.id(),"outcome":receipt["outcome"]}));
        }
        let value=json!({"complete":true,"revision":input.revision,"outcomes":outcomes,"drafts":drafts,"manual_approval_required":false});atomic_json(&path,&value)?;Ok(value)
    }.await;
    let _ = client.cancel().await;
    result
}

fn capture_time(
    item: &Value,
    turn: &Value,
    turn_id: &str,
) -> Result<Option<crate::chronology::SourceTime>> {
    use crate::chronology::{SourceTime, TimeBasis};
    for (object, key, basis) in [
        (item, "createdAt", TimeBasis::MessageTimestamp),
        (turn, "startedAt", TimeBasis::TurnTimestamp),
    ] {
        if let Some(value) = object.get(key) {
            let text = value
                .as_str()
                .ok_or("source dates must be RFC3339, never thread updatedAt or capture time")?;
            let time = chrono::DateTime::parse_from_rfc3339(text)
                .map_err(|_| "invalid source timestamp")?;
            if time > chrono::Utc::now() {
                return Err("source timestamp is in the future".into());
            }
            return Ok(Some(SourceTime {
                at: time
                    .with_timezone(&chrono::Utc)
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                basis,
            }));
        }
    }
    Ok(SourceTime::from_uuid(turn_id))
}
fn draft_identity(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(drafts) = value.as_array_mut() {
        for draft in drafts {
            if let Some(obj) = draft.as_object_mut() {
                obj.remove("assessment_token");
                obj.remove("related_assessments");
            }
        }
    }
    value
}
fn validate_assessments(lesson: &Lesson, draft: &ChatDraft, assessment: &Value) -> Result<()> {
    let related = assessment["related_lessons"]
        .as_array()
        .ok_or("missing chronology assessment")?;
    if draft.related_assessments.len() > 12 {
        return Err("at most twelve related-lesson assessments".into());
    }
    if !related.is_empty()
        && draft.assessment_token.as_deref() != assessment["assessment_token"].as_str()
    {
        return Err("existing same-key knowledge needs assessment: run assess_chat_lessons, read related lessons, and supply its token and related_assessments; retry identical lesson content".into());
    }
    let mut seen = BTreeSet::new();
    for a in &draft.related_assessments {
        if !seen.insert(&a.id) || !(15..=1000).contains(&a.reason.len()) || !safe_text(&a.reason) {
            return Err(
                "each related assessment needs a unique lesson ID and specific minimized reason"
                    .into(),
            );
        }
        let peer = related
            .iter()
            .find(|p| p["lesson"]["id"] == a.id)
            .ok_or("related assessment is stale or outside this key/repository")?;
        if matches!(a.relation, RelatedRelation::Correction) {
            let peer: Lesson = serde_json::from_value(peer["lesson"].clone())?;
            crate::chronology::guard_terminal(&lesson.conversation_evidence, &[peer])?;
        }
        if assessment["chronology"]["relation"] == "older_context"
            && !matches!(
                a.relation,
                RelatedRelation::Historical | RelatedRelation::Conflict
            )
        {
            return Err("older source must be assessed as historical or an unresolved conflict, not as current correction".into());
        }
    }
    if seen.len() != related.len() {
        return Err("assess every returned related lesson before publication".into());
    }
    Ok(())
}
pub async fn assess_lessons(settings: &Settings, input: RememberChatInput) -> Result<Value> {
    let lessons = build(settings, &input.repository, &input.revision, &input.lessons)?;
    let client = remote::connect(settings).await?;
    let result=async {
        if !remote::supports(&client,"assess_conversation_candidate").await?{return Err("upgrade Pi to schema 7 for chronological assessment".into());}
        let mut assessments=Vec::new();for lesson in lessons {let mut a=remote::call(&client,"assess_conversation_candidate",json!({"lesson":lesson})).await?;a["candidate_id"]=json!(lesson.id);assessments.push(a);}
        Ok(json!({"repository":input.repository,"revision":input.revision,"assessments":assessments,"guidance":"Read returned full records. Supply the token and a consistent/historical/conflict/correction assessment for every peer. Source chronology does not prove correctness. Existing keyless legacy advice needs separate recall."}))
    }.await;
    let _ = client.cancel().await;
    result
}
