//! Redacted, bounded operational reports. No raw errors, prompts or evidence bodies.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Capture,
    Publication,
    Reconciliation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Started,
    Succeeded,
    Failed,
    Skipped,
}
impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LearningRun {
    pub repository: String,
    pub id: String,
    pub phase: RunPhase,
    pub state: RunState,
    pub client_started_at: String,
    #[serde(default)]
    pub client_finished_at: Option<String>,
    #[serde(default)]
    pub pr: Option<u64>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub inserted: Option<usize>,
    #[serde(default)]
    pub reused: Option<usize>,
    #[serde(default)]
    pub changed: Option<usize>,
}
impl LearningRun {
    pub fn validate(&self) -> Result<(), &'static str> {
        crate::lesson::validate_repository(&self.repository)?;
        let digest = |s: &str| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit());
        if !digest(&self.id)
            || self.revision.as_deref().is_some_and(|s| !digest(s))
            || self.pr.is_some_and(|n| n == 0 || n > i64::MAX as u64)
            || self.inserted.is_some_and(|n| n > 10)
            || self.reused.is_some_and(|n| n > 10)
            || self.changed.is_some_and(|n| n > 10)
        {
            return Err("invalid run identity, source revision or counts");
        }
        if self.client_started_at.len() > 64
            || self
                .client_finished_at
                .as_ref()
                .is_some_and(|s| s.len() > 64)
        {
            return Err("invalid client timestamp size");
        }
        if matches!(self.state, RunState::Started | RunState::Failed)
            && (self.inserted.is_some() || self.reused.is_some() || self.changed.is_some())
        {
            return Err("incomplete/failed runs have unknown counts");
        }
        let start = chrono::DateTime::parse_from_rfc3339(&self.client_started_at)
            .map_err(|_| "invalid client start time")?;
        match (&self.client_finished_at, self.state) {
            (None, RunState::Started) => (),
            (Some(finish), state) if state != RunState::Started => {
                let finish = chrono::DateTime::parse_from_rfc3339(finish)
                    .map_err(|_| "invalid client completion time")?;
                if finish < start {
                    return Err("client completion precedes start");
                }
            }
            _ => return Err("started runs have no completion time; terminal reports need one"),
        }
        if self.state == RunState::Skipped && self.phase != RunPhase::Capture {
            return Err("only unchanged capture can be skipped");
        }
        Ok(())
    }
}
#[derive(Debug, Serialize)]
pub struct StoredRun {
    #[serde(flatten)]
    pub run: LearningRun,
    pub first_reported_at: String,
    pub reported_at: String,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunQuery {
    pub repository: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}
fn default_limit() -> usize {
    10
}
#[derive(Debug, Serialize)]
pub struct RunSummary {
    pub retained: u64,
    pub incomplete: u64,
    pub failures: u64,
    pub last_learning_report_at: Option<String>,
    pub last_failure_report_at: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct RunPage {
    pub repository: String,
    pub summary: RunSummary,
    pub runs: Vec<StoredRun>,
    pub next_offset: Option<usize>,
}
