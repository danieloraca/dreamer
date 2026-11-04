//! Bounded, deterministic retrieval. Lexical/context scores are not confidence.
use crate::lesson::{LessonStatus, StoredLesson, validate_repository};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LessonScope {
    pub components: Vec<String>,
    /// Repository-relative exact files or directory prefixes ending with '/'.
    pub paths: Vec<String>,
    /// Explicit task phrases for which this lesson should not be recalled.
    pub excluded_terms: Vec<String>,
}
impl LessonScope {
    pub fn is_empty(&self) -> bool {
        self.components.is_empty() && self.paths.is_empty() && self.excluded_terms.is_empty()
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.components.len() > 8
            || self.paths.len() > 12
            || self.excluded_terms.len() > 8
            || self
                .components
                .iter()
                .any(|s| s.trim().is_empty() || s != s.trim() || s.len() > 80)
            || self.paths.iter().any(|s| !valid_path(s))
            || self
                .excluded_terms
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 200)
        {
            return Err(
                "scope needs bounded components, relative file/directory paths and excluded phrases",
            );
        }
        Ok(())
    }
    pub fn canonical(&self) -> Self {
        fn sorted(items: &[String], fold: bool) -> Vec<String> {
            let mut result: Vec<_> = items
                .iter()
                .map(|s| {
                    if fold {
                        s.trim().to_ascii_lowercase()
                    } else {
                        s.trim().to_owned()
                    }
                })
                .collect();
            result.sort();
            result.dedup();
            result
        }
        Self {
            components: sorted(&self.components, true),
            paths: sorted(&self.paths, false),
            excluded_terms: sorted(&self.excluded_terms, true),
        }
    }
}
pub fn valid_path(path: &str) -> bool {
    !path.trim().is_empty()
        && path.len() <= 300
        && path == path.trim()
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '\0'])
        && path
            .trim_end_matches('/')
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecallQuery {
    pub repository: String,
    pub queries: Vec<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize {
    5
}
impl RecallQuery {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_repository(&self.repository)?;
        if !(1..=6).contains(&self.queries.len())
            || self
                .queries
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 200)
            || self.components.len() > 8
            || self
                .components
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 80)
            || self.paths.len() > 20
            || self.paths.iter().any(|s| !valid_path(s))
            || !(1..=20).contains(&self.limit)
        {
            return Err(
                "recall needs 1–6 short queries, bounded component/path context and limit 1–20",
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Relevance {
    pub score: usize,
    pub matched_queries: Vec<String>,
    pub matched_components: Vec<String>,
    pub matched_paths: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RecallHit {
    #[serde(flatten)]
    pub stored: StoredLesson,
    pub relevance: Relevance,
}
#[derive(Debug, Serialize)]
pub struct RecallPage {
    pub repository: String,
    pub lessons: Vec<RecallHit>,
    pub matched_count: usize,
    pub truncated: bool,
    pub candidate_limit_reached: bool,
}
fn contains_phrase(text: &str, phrase: &str) -> bool {
    let text = text.to_ascii_lowercase();
    let phrase = phrase.trim().to_ascii_lowercase();
    if phrase.is_empty() {
        return false;
    }
    let word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(&phrase).any(|(at, _)| {
        text[..at].chars().next_back().is_none_or(|c| !word(c))
            && text[at + phrase.len()..]
                .chars()
                .next()
                .is_none_or(|c| !word(c))
    })
}
fn positive_applicability(text: &str) -> String {
    let normalized = text.replace(". ", "; ");
    normalized
        .split([';', '\n'])
        .filter_map(|clause| {
            let clause = clause.trim().trim_start_matches(['-', '*']).trim();
            let lower = clause.to_ascii_lowercase();
            if [
                "not for ",
                "not applicable to ",
                "does not apply to ",
                "this does not apply to ",
                "excluding ",
                "except ",
                "this is not a ",
                "this is not an ",
            ]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
            {
                return None;
            }
            let end = lower.find(", not ").unwrap_or(clause.len());
            Some(clause[..end].to_owned())
        })
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn path_matches(scope: &str, path: &str) -> bool {
    scope == path || (scope.ends_with('/') && path.starts_with(scope))
}

pub fn rank(
    query: &RecallQuery,
    lessons: Vec<StoredLesson>,
    candidate_limit_reached: bool,
) -> Result<RecallPage, &'static str> {
    query.validate()?;
    let mut terms: Vec<_> = query
        .queries
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();
    terms.sort();
    terms.dedup();
    let mut hits = Vec::new();
    for stored in lessons {
        let lesson = &stored.lesson;
        if lesson.repository != query.repository || lesson.status == LessonStatus::Deprecated {
            continue;
        }
        if lesson.scope.excluded_terms.iter().any(|excluded| {
            query
                .queries
                .iter()
                .chain(&query.components)
                .any(|term| contains_phrase(term, excluded))
        }) {
            continue;
        }
        let summary = lesson.summary.to_ascii_lowercase();
        let applies = positive_applicability(&lesson.applies_when).to_ascii_lowercase();
        let mut score = 0;
        let mut matched_queries = Vec::new();
        for term in &terms {
            let points = usize::from(summary.contains(term)) * 8
                + usize::from(applies.contains(term)) * 3
                + usize::from(
                    lesson
                        .scope
                        .components
                        .iter()
                        .any(|c| c.to_ascii_lowercase().contains(term)),
                ) * 4
                + usize::from(
                    lesson
                        .scope
                        .paths
                        .iter()
                        .any(|p| p.to_ascii_lowercase().contains(term)),
                ) * 2;
            if points > 0 {
                matched_queries.push(term.clone());
                score += points;
            }
        }
        let matched_components: Vec<_> = query
            .components
            .iter()
            .filter(|c| {
                lesson
                    .scope
                    .components
                    .iter()
                    .any(|scope| scope.eq_ignore_ascii_case(c.trim()))
            })
            .cloned()
            .collect();
        let matched_paths: Vec<_> = query
            .paths
            .iter()
            .filter(|path| {
                lesson
                    .scope
                    .paths
                    .iter()
                    .any(|scope| path_matches(scope, path))
            })
            .cloned()
            .collect();
        let specificity = lesson
            .scope
            .paths
            .iter()
            .filter(|scope| query.paths.iter().any(|path| path_matches(scope, path)))
            .map(|scope| {
                scope.trim_end_matches('/').split('/').count().min(4) * 2
                    + usize::from(query.paths.iter().any(|path| path == scope)) * 4
            })
            .max()
            .unwrap_or(0);
        score += usize::from(!matched_components.is_empty()) * 8
            + usize::from(!matched_paths.is_empty()) * 12
            + specificity;
        if score > 0 {
            hits.push(RecallHit {
                stored,
                relevance: Relevance {
                    score,
                    matched_queries,
                    matched_components,
                    matched_paths,
                },
            });
        }
    }
    hits.sort_by(|a, b| {
        b.relevance
            .score
            .cmp(&a.relevance.score)
            .then_with(|| a.stored.lesson.id.cmp(&b.stored.lesson.id))
    });
    let matched_count = hits.len();
    let truncated = matched_count > query.limit || candidate_limit_reached;
    hits.truncate(query.limit);
    Ok(RecallPage {
        repository: query.repository.clone(),
        lessons: hits,
        matched_count,
        truncated,
        candidate_limit_reached,
    })
}
