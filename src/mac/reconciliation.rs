//! Constrained automatic reconciliation from the same inspected PR evidence as learning.
use super::{
    Result, Settings,
    evidence::{self, Draft},
    remote,
};
use crate::{
    collector::github::GitHubApi,
    lifecycle::{ChangeAction, LessonChange},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Duration};

#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeDraft {
    pub source_id: String,
    pub action: ChangeAction,
    #[serde(default)]
    pub target_id: Option<String>,
    pub reason: String,
    /// Inspected review/discussion and final implementation/test source IDs.
    pub evidence_ids: Vec<String>,
}
#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReconcileInput {
    pub revision: String,
    /// Omit to resume this revision's saved pending batch.
    #[serde(default)]
    pub changes: Vec<ChangeDraft>,
}

pub async fn reconcile(
    settings: &Settings,
    api: &impl GitHubApi,
    mut input: ReconcileInput,
) -> Result<Value> {
    let path = evidence::reconciliation_path(settings, &input.revision)?;
    let prior = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    if input.changes.is_empty() {
        let pending = prior
            .as_ref()
            .filter(|journal| journal["revision"] == input.revision && journal["complete"] == false)
            .ok_or("no pending batch; supply 1–10 evidence-backed changes")?;
        input.changes = serde_json::from_value(pending["drafts"].clone()).map_err(
            |_| "pending drafts unavailable; inspect the private reconciliation journal",
        )?;
    }
    if input.changes.is_empty() || input.changes.len() > 10 {
        return Err("reconcile needs 1–10 evidence-backed changes".into());
    }
    let drafts = serde_json::to_value(&input.changes)?;
    let pending = prior
        .as_ref()
        .filter(|journal| journal["complete"] == false && journal["revision"] == input.revision);
    let writes_started = pending.is_some_and(|journal| {
        journal["writes_started"] == true || journal.get("in_flight").is_some()
    });
    if writes_started && pending.is_some_and(|journal| journal["drafts"] != drafts) {
        return Err(
            "resume the saved pending batch before starting different changes on this revision"
                .into(),
        );
    }
    // Reuse the pinned-source/read receipt and review-to-final-path safeguards.
    let proofs: Vec<_> = input
        .changes
        .iter()
        .map(|draft| Draft {
            summary: draft.reason.clone(),
            applies_when: format!(
                "Reconcile {} for lesson {} toward {:?} using this PR's inspected evidence.",
                draft.action.as_str(),
                draft.source_id,
                draft.target_id
            ),
            evidence_ids: draft.evidence_ids.clone(),
        })
        .collect();
    let proofs = evidence::build_lessons(settings, &input.revision, &proofs)?;
    let snapshot = evidence::check_current(settings, api, &input.revision)?;
    let mut changes = Vec::new();
    let mut identities = BTreeSet::new();
    for (draft, proof) in input.changes.iter().cloned().zip(proofs) {
        let change=LessonChange {repository:snapshot.repository.clone(),source_id:draft.source_id,action:draft.action,target_id:draft.target_id,reason:draft.reason,
            producer:"Codex agent via Dreamer Mac; source-backed reconciliation, not independently verified".into(),evidence:proof.evidence}.canonical();
        change.validate()?;
        if !identities.insert(change.id()) {
            return Err("duplicate lifecycle change in batch".into());
        }
        changes.push(change);
    }
    // Keep uncertain write intent intact while reconnecting. Before any write has
    // started, a rejected preflight can be corrected without trapping the PR.
    if !writes_started {
        evidence::atomic_json(
            &path,
            &json!({"complete":false,"revision":input.revision,"drafts":drafts,"outcomes":[],"writes_started":false}),
        )?;
    }
    let client = remote::connect(settings).await?;
    let result=async {
        let tools=tokio::time::timeout(Duration::from_secs(30),client.list_all_tools()).await.map_err(|_|"Pi tool discovery timed out")?.map_err(|_|"Pi tool discovery failed")?;
        for name in ["change_lesson","get_lesson_history"] {
            if !tools.iter().any(|tool|tool.name==name) {return Err("Pi upgrade required: run make deploy before using lesson reconciliation".into());}
        }
        // Preflight the entire batch, including dependencies between planned changes.
        let mut completed=BTreeSet::new();
        let mut retired=BTreeSet::new();
        for change in &changes {
            let prior=remote::call(&client,"get_lesson_history",json!({"repository":change.repository,"id":change.source_id,"change_id":change.id(),"limit":1})).await?;
            let history=prior["changes"].as_array().ok_or("missing lifecycle history")?;
            if let Some(record)=history.first() {
                if record["id"]!=change.id() || record["change"]!=serde_json::to_value(change)? {return Err("stored change identity conflict".into());}
                completed.insert(change.id());
                continue;
            }
            for id in std::iter::once(&change.source_id).chain(change.target_id.iter()) {
                if retired.contains(id) {return Err("batch references a lesson retired earlier in this batch; select its active successor".into());}
                let stored=remote::call(&client,"get_lesson",json!({"repository":change.repository,"id":id})).await?;
                if stored["found"]!=true || stored["lesson"]["lesson"]["repository"]!=change.repository || stored["lesson"]["lesson"]["id"]!=*id {return Err("source or target lesson not found in this repository".into());}
                if stored["lesson"]["lesson"]["status"]=="deprecated" {return Err("source or target already retired/superseded; inspect its history".into());}
            }
            if change.action!=ChangeAction::Conflict {retired.insert(change.source_id.clone());}
        }
        let mut outcomes=Vec::new();
        evidence::atomic_json(&path,&json!({"complete":false,"revision":input.revision,"drafts":drafts,"outcomes":outcomes,"writes_started":writes_started}))?;
        for change in &changes {
            let id=change.id();
            if completed.contains(&id) {outcomes.push(json!({"id":id,"outcome":"already_applied"}));continue;}
            evidence::atomic_json(&path,&json!({"complete":false,"revision":input.revision,"drafts":drafts,"outcomes":outcomes,"writes_started":true,"in_flight":id}))?;
            let acknowledged=remote::call(&client,"change_lesson",json!({"change":change})).await?;
            if !matches!(acknowledged["outcome"].as_str(),Some("applied"|"unchanged")) || acknowledged["record"]["change"]!=serde_json::to_value(change)? || acknowledged["record"]["id"]!=id {return Err("unexpected change acknowledgement; retry identical changes".into());}
            let readback=remote::call(&client,"get_lesson_history",json!({"repository":change.repository,"id":change.source_id,"change_id":id,"limit":1})).await?;
            if readback["changes"][0]!=acknowledged["record"] {return Err("change readback differed; retry identical changes".into());}
            outcomes.push(json!({"id":id,"outcome":acknowledged["outcome"]}));
        }
        let mut reply=json!({"complete":true,"revision":input.revision,"outcomes":outcomes,"manual_approval_required":false});
        let mut journal=reply.clone();
        journal["drafts"]=drafts;
        evidence::atomic_json(&path,&journal)?;
        let learning_complete=evidence::finish_reconciliation(settings,&snapshot,&input.revision)?;
        reply["learning_complete"]=json!(learning_complete);
        if !learning_complete {reply["guidance"]=json!("Reconciliation completed, but candidate publication is incomplete. Retry the same candidate drafts before treating the PR as fully processed.");}
        Ok(reply)
    }.await;
    let _ = client.cancel().await;
    result
}
