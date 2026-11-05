//! Local fragments and bounded evidence bundles. The Pi wire format stays unchanged.
use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Fragment {
    pub start: usize,
    pub end: usize,
    pub view_bytes: usize,
    pub span_sha256: String,
}

pub(super) fn fragments(item: Item, text: &str) -> Vec<Item> {
    if text.len() <= 1600 {
        return vec![item];
    }
    let mut result = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + 1600).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end < text.len()
            && let Some(at) = text[start..end].rfind('\n').filter(|at| *at >= 1000)
        {
            end = start + at + 1;
        }
        let span = &text[start..end];
        let mut fragment = item.clone();
        fragment.source_item_id = Some(item.id.clone());
        fragment.id = format!(
            "fragment-{}",
            &collector::digest(
                format!("{}:{}:{start}:{end}", item.id, item.content_sha256).as_bytes()
            )[..40]
        );
        fragment.text = span.into();
        fragment.fragment = Some(Fragment {
            start,
            end,
            view_bytes: text.len(),
            span_sha256: collector::digest(span.as_bytes()),
        });
        result.push(fragment);
        if end == text.len() {
            break;
        }
        start = end - 160;
        while !text.is_char_boundary(start) {
            start += 1;
        }
    }
    result
}

pub(super) fn description(item: &Item) -> String {
    let base = "Agent-inspected sanitized conversation excerpt. Source assertions are not independently verified; recorded permissions do not authorize new operations.";
    match &item.fragment {
        None => base.into(),
        Some(f) => format!(
            "{base} Inspected sanitized-view UTF-8 bytes {}..{} of {}; digest identifies the complete sanitized message view.",
            f.start, f.end, f.view_bytes
        ),
    }
}

/// Stage inspected proposals locally; does not publish or mark them learned.
pub fn stage(settings: &Settings, input: RememberChatInput) -> Result<Value> {
    let s = load(settings, &input.repository, &input.revision)?;
    let dir = folder(settings, &input.revision)?;
    let _guard = lock(&dir.join(".publication.lock"))?;
    if pending_reconciliation(&dir)?
        || optional_json::<Value>(&dir.join("publication.json"))?.is_some()
    {
        return Err("finish existing publication/corrections before staging this chunk".into());
    }
    if input.lessons.len() > 3 {
        return Err(
            "stage at most three proposals per chunk; consolidate within a bounded window".into(),
        );
    }
    let keys: BTreeSet<_> = input
        .lessons
        .iter()
        .filter_map(|d| d.knowledge_key.as_deref())
        .collect();
    if keys.len() != input.lessons.len() {
        return Err(
            "stage one proposal per narrow knowledge_key; consolidate repeated claims first".into(),
        );
    }
    build(settings, &input.repository, &input.revision, &input.lessons)?;
    if input.lessons.is_empty() {
        let reason = input
            .skip_reason
            .as_deref()
            .ok_or("empty stage needs a specific skip_reason")?;
        if !(15..=1000).contains(&reason.len()) || !safe_text(reason) {
            return Err("use a bounded sanitized skip reason".into());
        }
    } else if input.skip_reason.is_some() {
        return Err("skip_reason applies only to an empty batch".into());
    }
    let value = json!({"staged":true,"complete":true,"published":false,"repository":s.repository,"thread_id":s.thread_id,"revision":input.revision,"drafts":input.lessons,"skip_reason":input.skip_reason,"guidance":"Staged proposals are not Pi knowledge. Consolidate duplicate claim/applicability keys, recall stored advice, bundle selected original evidence and assess before publication. Complete bounded windows before the cache fills."});
    if let Some(prior) = optional_json::<Value>(&dir.join("staged.json"))? {
        if prior["drafts"] != value["drafts"] || prior["skip_reason"] != value["skip_reason"] {
            return Err("staged proposals are immutable; resolve changed conclusions explicitly during consolidation".into());
        }
        return Ok(prior);
    }
    atomic_json(&dir.join("staged.json"), &value)?;
    Ok(value)
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionAction {
    Include,
    Skip,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposalResolution {
    pub revision: String,
    pub knowledge_key: String,
    pub action: ResolutionAction,
    pub reason: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatBundleInput {
    pub repository: String,
    pub thread_id: String,
    pub revisions: Vec<String>,
    /// Internal evidence/fragment IDs, never invented source message IDs.
    pub evidence_ids: Vec<String>,
    /// Account for every staged proposal; no silent loss when a window completes.
    #[serde(default)]
    pub resolutions: Vec<ProposalResolution>,
}

pub fn bundle(settings: &Settings, input: ChatBundleInput) -> Result<Value> {
    settings.select_repository(Some(&input.repository))?;
    if input.revisions.is_empty() || input.revisions.len() > 12 || input.evidence_ids.len() > 60 {
        return Err("bundle 1–12 staged revisions and at most 60 selected evidence IDs".into());
    }
    let r = root(settings)?;
    let _ = r;
    let wanted: BTreeSet<_> = input.evidence_ids.iter().cloned().collect();
    if wanted.len() != input.evidence_ids.len() {
        return Err("duplicate bundle evidence ID".into());
    }
    let mut items = BTreeMap::new();
    let mut parents = BTreeSet::new();
    let mut resolved = BTreeSet::new();
    for resolution in &input.resolutions {
        if !(15..=1000).contains(&resolution.reason.len())
            || !safe_text(&resolution.reason)
            || !resolved.insert((
                resolution.revision.clone(),
                resolution.knowledge_key.clone(),
            ))
        {
            return Err(
                "unique bounded sanitized disposition required for every staged proposal".into(),
            );
        }
    }
    let mut expected = BTreeSet::new();
    let mut inspected = BTreeSet::new();
    for revision in &input.revisions {
        if !parents.insert(revision.clone()) {
            return Err("duplicate bundle revision".into());
        }
        let source = load(settings, &input.repository, revision)?;
        let stale = current(settings, &source).is_err();
        if source.thread_id != input.thread_id || !source.parents.is_empty() {
            return Err("bundle only original chunks of the selected thread".into());
        }
        let dir = folder(settings, revision)?;
        if pending_reconciliation(&dir)?
            || optional_json::<Value>(&dir.join("publication.json"))?
                .is_some_and(|v| v["complete"] != true)
        {
            return Err("finish pending source corrections/publications before bundling".into());
        }
        let staged = optional_json::<Value>(&dir.join("staged.json"))?;
        if staged.as_ref().is_none_or(|v| v["complete"] != true) {
            return Err("stage every chunk before bundling".into());
        }
        for draft in staged.as_ref().unwrap()["drafts"]
            .as_array()
            .ok_or("invalid staged proposals")?
        {
            let key = draft["knowledge_key"]
                .as_str()
                .ok_or("staged claim needs a key")?;
            expected.insert((revision.clone(), key.to_owned()));
            let resolution = input
                .resolutions
                .iter()
                .find(|r| r.revision == *revision && r.knowledge_key == key)
                .ok_or("explicit include/skip disposition required for every staged proposal")?;
            if resolution.action == ResolutionAction::Include
                && draft["evidence_ids"]
                    .as_array()
                    .ok_or("missing staged citations")?
                    .iter()
                    .any(|id| id.as_str().is_none_or(|id| !wanted.contains(id)))
            {
                return Err("include a proposal's inspected sources in its evidence bundle, or explicitly skip it".into());
            }
        }
        if stale
            && (source.items.iter().any(|i| wanted.contains(&i.id))
                || input
                    .resolutions
                    .iter()
                    .any(|r| r.revision == *revision && r.action == ResolutionAction::Include))
        {
            return Err(
                "stale source can only be explicitly skipped, never cited or included".into(),
            );
        }
        if !stale {
            let read: BTreeSet<String> = optional_json(&dir.join("read.json"))?.unwrap_or_default();
            inspected.extend(read.into_iter().filter(|id| wanted.contains(id)));
        }
        for item in source.items.into_iter().filter(|i| wanted.contains(&i.id)) {
            if let Some(previous) = items.insert(item.id.clone(), item.clone())
                && serde_json::to_value(previous)? != serde_json::to_value(item)?
            {
                return Err("conflicting source views in bundle".into());
            }
        }
    }
    if wanted.is_empty()
        && input
            .resolutions
            .iter()
            .any(|r| r.action == ResolutionAction::Include)
    {
        return Err("included proposals require inspected evidence".into());
    }
    if expected != resolved {
        return Err("bundle dispositions must match its staged proposals exactly".into());
    }
    if items.len() != wanted.len() {
        return Err("bundle evidence must belong to its source chunks".into());
    }
    if items
        .values()
        .map(|i| &i.turn_id)
        .collect::<BTreeSet<_>>()
        .len()
        > 20
    {
        return Err("bundle at most 20 source turns".into());
    }
    let value = capture(
        settings,
        Snapshot {
            repository: input.repository,
            thread_id: input.thread_id,
            observed_at: now(),
            items: items.into_values().collect(),
            next_cursor: None,
            has_more: false,
            next_fragment_offset: None,
            parents: parents.into_iter().collect(),
            item_pagination: false,
            fragmented: false,
            resolutions: input.resolutions,
        },
    )?;
    let dir = folder(
        settings,
        value["revision"]
            .as_str()
            .ok_or("missing bundle revision")?,
    )?;
    let _guard = lock(&dir.join(".lock"))?;
    let mut existing: BTreeSet<String> = optional_json(&dir.join("read.json"))?.unwrap_or_default();
    existing.extend(inspected);
    atomic_json(&dir.join("read.json"), &existing)?;
    Ok(value)
}

pub(super) fn completed_for_reader(
    settings: &Settings,
    repo: &str,
    revision: &str,
) -> Result<bool> {
    let s = load(settings, repo, revision)?;
    current(settings, &s)?;
    let dir = folder(settings, revision)?;
    if pending_reconciliation(&dir)? {
        return Ok(false);
    }
    let publication = optional_json::<Value>(&dir.join("publication.json"))?;
    if let Some(v) = publication {
        return Ok(v["complete"] == true);
    }
    Ok(receipt_complete(&dir, "staged.json")? || receipt_complete(&dir, "stage-consumed.json")?)
}
