//! Future Mac-side read-only GitHub adapter. No HTTP client or authentication yet.
//! The Pi will receive knowledge through a future service/MCP boundary.

use crate::lesson::Evidence;

#[derive(Debug, Clone)]
pub struct PullRequestSnapshot {
    pub repository: String,
    pub number: u64,
    /// The final revision against which evidence must be checked.
    pub head_sha: String,
    pub title: String,
    pub diff: String,
    pub review_comments: Vec<Evidence>,
}

/// Implement pagination, merged-PR filtering and a persisted cursor in the adapter.
pub trait PullRequestSource {
    type Error: std::error::Error;

    fn fetch_merged(
        &self,
        repository: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PullRequestPage, Self::Error>;
}

#[derive(Debug)]
pub struct PullRequestPage {
    pub items: Vec<PullRequestSnapshot>,
    pub next_cursor: Option<String>,
}
