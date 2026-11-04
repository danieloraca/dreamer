//! Repository-scoped storage port and local SQLite implementation.

pub mod sqlite;

use crate::lesson::{Lesson, StoredLesson};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    Inserted,
    Unchanged,
}

#[derive(Debug)]
pub struct SearchQuery {
    pub repository: String,
    pub text: String,
    pub limit: usize,
}

/// Search must stay within the requested repository and exclude deprecated lessons.
/// Return status and evidence with each result so callers can assess reliability.
pub trait LessonStore {
    type Error: std::error::Error;

    fn put_candidate(&mut self, lesson: &Lesson) -> Result<WriteOutcome, Self::Error>;
    fn get(&self, repository: &str, id: &str) -> Result<Option<StoredLesson>, Self::Error>;
    fn search(&self, query: &SearchQuery) -> Result<Vec<StoredLesson>, Self::Error>;
}
