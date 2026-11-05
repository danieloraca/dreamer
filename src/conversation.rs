//! Minimal conversation provenance. Excerpts stay on the Mac, never in these rows.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    UserStatement,
    AssistantReport,
    ObservedResult,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationClaim {
    Decision,
    Correction,
    ObservedOutcome,
    Hypothesis,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationEvidence {
    pub provider: String,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub source_url: String,
    pub observed_at: String,
    pub content_sha256: String,
    pub kind: ConversationKind,
    pub claim: ConversationClaim,
    /// Always an excerpt/view, not a claim to have archived the complete chat.
    pub description: String,
}
pub fn valid_thread(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}
pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_:".contains(&c))
}
pub fn digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}
impl ConversationEvidence {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.provider != "codex"
            || !valid_thread(&self.thread_id)
            || ![&self.thread_id, &self.turn_id, &self.item_id]
                .iter()
                .all(|s| valid_id(s))
            || self.source_url != format!("codex://threads/{}", self.thread_id)
            || !digest(&self.content_sha256)
            || !(15..=1000).contains(&self.description.trim().len())
            || self.observed_at.len() > 64
            || chrono::DateTime::parse_from_rfc3339(&self.observed_at).is_err()
        {
            return Err(
                "chat evidence needs Codex thread/turn/item identity, exact thread link, digest, observation time and bounded description",
            );
        }
        Ok(())
    }
}
