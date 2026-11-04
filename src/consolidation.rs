//! Reserved standalone extraction port. The on-demand Mac MCP workflow uses the
//! coding agent for extraction and publishes sourced candidates automatically.

use crate::{ingest::github::PullRequestSnapshot, lesson::Lesson};

/// Implementations must emit candidate lessons; candidate status does not imply a human approval queue.
pub trait Consolidator {
    type Error: std::error::Error;

    fn propose(&self, source: &PullRequestSnapshot) -> Result<Vec<Lesson>, Self::Error>;
}
