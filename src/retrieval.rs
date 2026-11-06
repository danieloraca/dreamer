//! Bounded, deterministic retrieval. Lexical/context scores are not confidence.
use crate::lesson::{LessonStatus, StoredLesson, validate_repository};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
    pub matched_tokens: Vec<String>,
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
    pub relevance_policy: &'static str,
    pub outcome: &'static str,
}
fn contains_phrase(text: &str, phrase: &str) -> bool {
    let text = word_sequence(text);
    let phrase = word_sequence(phrase);
    if phrase.is_empty() {
        return false;
    }
    text.windows(phrase.len()).any(|part| part == phrase)
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

/// Small English inflection heuristic, not fuzzy/semantic matching. Keep this
/// identical for task words and lesson words; exclusions retain phrase semantics.
fn canonical_word(word: &str) -> String {
    let word = word.to_ascii_lowercase();
    let singular = if word.len() > 5 && word.ends_with("ies") {
        format!("{}y", &word[..word.len() - 3])
    } else if word.len() > 4
        && word.ends_with('s')
        && !["ss", "us", "is"]
            .iter()
            .any(|suffix| word.ends_with(suffix))
    {
        word[..word.len() - 1].to_owned()
    } else {
        word
    };
    if singular.len() >= 7 && singular.ends_with("ing") {
        singular[..singular.len() - 3].to_owned()
    } else {
        singular
    }
}

fn word_sequence(text: &str) -> Vec<String> {
    let chars: Vec<_> = text.chars().collect();
    let mut separated = String::new();
    for (i, &c) in chars.iter().enumerate() {
        // BotResponse and HTTPResponse both yield independent words.
        if !c.is_alphanumeric()
            || (c.is_ascii_uppercase()
                && i > 0
                && (chars[i - 1].is_ascii_lowercase()
                    || chars[i - 1].is_ascii_digit()
                    || (chars[i - 1].is_ascii_uppercase()
                        && chars.get(i + 1).is_some_and(char::is_ascii_lowercase))))
        {
            separated.push(' ');
        }
        if c.is_alphanumeric() {
            separated.push(c);
        }
    }
    separated.split_whitespace().map(canonical_word).collect()
}

fn words(text: &str) -> BTreeSet<String> {
    word_sequence(text).into_iter().collect()
}

fn query_groups(query: &RecallQuery) -> BTreeMap<Vec<String>, String> {
    let mut groups = BTreeMap::new();
    for text in &query.queries {
        let tokens: Vec<_> = words(text)
            .into_iter()
            .filter(|s| {
                !matches!(
                    s.as_str(),
                    "a" | "an"
                        | "and"
                        | "are"
                        | "as"
                        | "at"
                        | "be"
                        | "by"
                        | "for"
                        | "from"
                        | "how"
                        | "in"
                        | "is"
                        | "it"
                        | "of"
                        | "on"
                        | "or"
                        | "that"
                        | "the"
                        | "this"
                        | "to"
                        | "was"
                        | "when"
                        | "with"
                )
            })
            .collect();
        if !tokens.is_empty() {
            // Duplicate/reordered phrases must not multiply relevance scores.
            let normalized = text.trim().to_ascii_lowercase();
            groups
                .entry(tokens)
                .and_modify(|old: &mut String| {
                    if normalized < *old {
                        *old = normalized.clone();
                    }
                })
                .or_insert(normalized);
        }
    }
    groups
}

/// SQL needles are a superset of complete word-group matches. Each inner
/// group requires all needles; different groups are alternatives.
pub(crate) fn candidate_groups(query: &RecallQuery) -> Vec<Vec<String>> {
    query_groups(query)
        .into_keys()
        .map(|group| {
            group
                .into_iter()
                .map(|word| {
                    // replies -> reply; both must remain selectable by the SQL prefilter.
                    if word.len() > 3 && word.ends_with('y') {
                        word[..word.len() - 1].to_owned()
                    } else {
                        word
                    }
                })
                .collect()
        })
        .collect()
}

pub fn rank(
    query: &RecallQuery,
    lessons: Vec<StoredLesson>,
    candidate_limit_reached: bool,
) -> Result<RecallPage, &'static str> {
    query.validate()?;
    let groups = query_groups(query);
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
        let fields = [
            (words(&lesson.summary), 8),
            (words(&positive_applicability(&lesson.applies_when)), 3),
            (words(&lesson.scope.components.join(" ")), 4),
            (words(&lesson.scope.paths.join(" ")), 2),
        ];
        let mut score = 0;
        let mut matched_queries = Vec::new();
        let mut matched_tokens = BTreeSet::new();
        for (tokens, original) in &groups {
            let matches: Vec<_> = tokens
                .iter()
                .filter(|token| fields.iter().any(|(set, _)| set.contains(*token)))
                .collect();
            // Require the whole task group. Partial topical overlap must not
            // introduce advice about a different operation in the same feature.
            if matches.len() == tokens.len() {
                matched_queries.push(original.clone());
                score += usize::from(tokens.len() > 1 && matches.len() == tokens.len()) * 8;
                for token in matches {
                    // Overlapping query groups do not count the same word twice.
                    if matched_tokens.insert(token.clone()) {
                        score += fields
                            .iter()
                            .filter(|(set, _)| set.contains(token))
                            .map(|(_, weight)| weight)
                            .sum::<usize>();
                    }
                }
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
        // Scope is an applicability constraint when comparable task context is
        // supplied. A shared broad component cannot override a path mismatch.
        // Include known affected consumers in query.paths for shared-code work.
        let path_mismatch =
            !query.paths.is_empty() && !lesson.scope.paths.is_empty() && matched_paths.is_empty();
        let component_mismatch = !query.components.is_empty()
            && !lesson.scope.components.is_empty()
            && matched_components.is_empty()
            && matched_paths.is_empty();
        if matched_queries.is_empty() || path_mismatch || component_mismatch {
            continue;
        }
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
                    matched_tokens: matched_tokens.into_iter().collect(),
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
            .then_with(|| {
                crate::chronology::lesson_range(&b.stored.lesson)
                    .map(|(_, t)| t)
                    .cmp(&crate::chronology::lesson_range(&a.stored.lesson).map(|(_, t)| t))
            })
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
        relevance_policy: "strict_v1",
        outcome: if matched_count == 0 {
            "no_applicable_match"
        } else {
            "matches"
        },
    })
}
