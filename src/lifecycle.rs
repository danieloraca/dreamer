//! Immutable evidence-backed lesson transitions; status is a projection of history.
use crate::lesson::{Evidence, ExtractionMethod, Lesson, LessonStatus, Provenance};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAction {
    Supersede,
    Retire,
    Conflict,
}
impl ChangeAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supersede => "supersede",
            Self::Retire => "retire",
            Self::Conflict => "conflict",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LessonChange {
    pub repository: String,
    pub source_id: String,
    pub action: ChangeAction,
    #[serde(default)]
    pub target_id: Option<String>,
    pub reason: String,
    /// Asserted producer, not an authenticated identity or verification claim.
    pub producer: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversation_evidence: Vec<crate::conversation::ConversationEvidence>,
}
impl LessonChange {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.source_id.len() > 1000
            || !(15..=2000).contains(&self.reason.trim().len())
            || !(1..=500).contains(&self.producer.trim().len())
            || !(1..=12).contains(&(self.evidence.len() + self.conversation_evidence.len()))
        {
            return Err(
                "change needs bounded identity, reason, producer and 1–12 evidence sources",
            );
        }
        match (self.action, self.target_id.as_deref()) {
            (ChangeAction::Retire, None) => (),
            (ChangeAction::Supersede | ChangeAction::Conflict, Some(target))
                if !target.trim().is_empty()
                    && target.len() <= 1000
                    && target != self.source_id => {}
            _ => {
                return Err(
                    "retire has no target; supersede/conflict need a different target lesson",
                );
            }
        }
        // Reuse the existing repository and pinned PR-evidence invariants.
        Lesson {
            id: self.source_id.clone(),
            repository: self.repository.clone(),
            summary: self.reason.clone(),
            applies_when: "Lesson lifecycle evidence".into(),
            status: LessonStatus::Candidate,
            scope: Default::default(),
            knowledge_key: None,
            provenance: Provenance {
                producer: self.producer.clone(),
                method: ExtractionMethod::Manual,
            },
            evidence: self.evidence.clone(),
            conversation_evidence: self.conversation_evidence.clone(),
        }
        .validate_candidate()
    }
    /// Canonicalize symmetric conflict pairs and incidental surrounding whitespace.
    pub fn canonical(&self) -> Self {
        let mut change = self.clone();
        change.reason = change.reason.trim().into();
        change.producer = change.producer.trim().into();
        if change.action == ChangeAction::Conflict
            && let Some(target) = &mut change.target_id
            && *target < change.source_id
        {
            std::mem::swap(target, &mut change.source_id);
        }
        change
    }
    pub fn id(&self) -> String {
        format!(
            "chg-{}",
            crate::collector::digest(
                &serde_json::to_vec(&self.canonical()).expect("change serialization")
            )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredChange {
    pub id: String,
    pub change: LessonChange,
    pub stored_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retirement_reason: Option<String>,
    pub conflicts: Vec<String>,
    pub conflicts_truncated: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryQuery {
    pub repository: String,
    pub id: String,
    /// Optional exact event identity for retry/readback.
    #[serde(default)]
    pub change_id: Option<String>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "history_limit")]
    pub limit: usize,
}
fn history_limit() -> usize {
    20
}
#[derive(Debug, Serialize)]
pub struct HistoryPage {
    pub repository: String,
    pub id: String,
    pub changes: Vec<StoredChange>,
    pub next_offset: Option<usize>,
}
