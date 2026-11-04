//! Mac-side GitHub collection types. No GitHub credential is needed on the Pi.

pub use crate::collector::Snapshot as PullRequestSnapshot;
pub use crate::collector::github::{GhApi, GitHubApi};
