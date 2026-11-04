use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LessonStatus {
    Candidate,
    Verified,
    Deprecated,
}

/// Source pointers are evidence to inspect, never instructions to execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PullRequestRef {
    pub number: u64,
    pub head_sha: String,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMethod {
    Manual,
    Fixture,
}

/// Who proposed a lesson and how; this is an assertion, not verified identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub producer: String,
    pub method: ExtractionMethod,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub source_url: String,
    pub commit_sha: Option<String>,
    pub description: String,
    pub pull_request: PullRequestRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
    pub id: String,
    pub repository: String,
    pub summary: String,
    pub applies_when: String,
    pub status: LessonStatus,
    pub provenance: Provenance,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredLesson {
    pub lesson: Lesson,
    /// UTC timestamp assigned by SQLite, independent of producer claims.
    pub stored_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<crate::lifecycle::LifecycleSummary>,
}

pub fn validate_repository(repository: &str) -> Result<(), &'static str> {
    let parts: Vec<_> = repository.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || !part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
    {
        return Err("repository must use owner/repository syntax");
    }
    Ok(())
}

fn valid_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|c| c.is_ascii_hexdigit())
}

impl Lesson {
    pub fn validate_candidate(&self) -> Result<(), &'static str> {
        self.validate()?;
        if self.status != LessonStatus::Candidate {
            return Err("only candidate lessons may be imported");
        }
        Ok(())
    }

    /// Minimum validation for future ingestion and storage boundaries.
    /// This checks completeness, not factual truth or verification authority.
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_repository(&self.repository)?;
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
        if self.provenance.producer.trim().is_empty() {
            return Err("lesson provenance requires a producer");
        }
        if self.evidence.is_empty()
            || self
                .evidence
                .iter()
                .any(|item| item.source_url.trim().is_empty() || item.description.trim().is_empty())
        {
            return Err("a lesson must include source pointers and evidence descriptions");
        }
        for evidence in &self.evidence {
            let pr = &evidence.pull_request;
            if pr.number == 0
                || pr.number > i64::MAX as u64
                || pr.title.trim().is_empty()
                || pr.url.trim().is_empty()
                || !valid_sha(&pr.head_sha)
                || evidence
                    .commit_sha
                    .as_deref()
                    .is_some_and(|sha| !valid_sha(sha))
            {
                return Err(
                    "PR evidence requires a positive number, title, URL and full 40-character commit SHAs",
                );
            }
        }
        Ok(())
    }
}
