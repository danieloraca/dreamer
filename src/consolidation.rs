//! Future model-assisted extraction. No model calls or automatic verification.

use crate::{ingest::github::PullRequestSnapshot, lesson::Lesson};

/// Implementations must emit candidate lessons; verification is a separate step.
pub trait Consolidator {
    type Error: std::error::Error;

    fn propose(&self, source: &PullRequestSnapshot) -> Result<Vec<Lesson>, Self::Error>;
}
