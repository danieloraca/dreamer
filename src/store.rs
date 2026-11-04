//! Storage port. A persistent SQLite implementation and migrations are pending.

use crate::lesson::Lesson;

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

    fn put_candidate(&mut self, lesson: &Lesson) -> Result<(), Self::Error>;
    fn get(&self, repository: &str, id: &str) -> Result<Option<Lesson>, Self::Error>;
    fn search(&self, query: &SearchQuery) -> Result<Vec<Lesson>, Self::Error>;
}
