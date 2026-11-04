use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonStatus {
    Candidate,
    Verified,
    Deprecated,
}

/// Source pointers are evidence to inspect, never instructions to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub source_url: String,
    pub commit_sha: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub repository: String,
    pub summary: String,
    pub applies_when: String,
    pub status: LessonStatus,
    pub evidence: Vec<Evidence>,
}

impl Lesson {
    /// Minimum validation for future ingestion and storage boundaries.
    /// This checks completeness, not factual truth or verification authority.
    pub fn validate(&self) -> Result<(), &'static str> {
        if [
            &self.id,
            &self.repository,
            &self.summary,
            &self.applies_when,
        ]
        .iter()
        .any(|value| value.trim().is_empty())
        {
            return Err("lesson identity, scope, summary and applicability are required");
        }
        if self.evidence.is_empty()
            || self
                .evidence
                .iter()
                .any(|item| item.source_url.trim().is_empty() || item.description.trim().is_empty())
        {
            return Err("a lesson must include source pointers and evidence descriptions");
        }
        Ok(())
    }
}
