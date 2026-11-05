//! Source chronology is independent of ingestion order and never proves truth.
use crate::{
    conversation::{ConversationClaim, ConversationEvidence},
    lesson::Lesson,
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeBasis {
    MessageTimestamp,
    TurnTimestamp,
    UuidV7TurnStart,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceTime {
    pub at: String,
    pub basis: TimeBasis,
}
impl SourceTime {
    pub fn datetime(&self) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.at)
            .ok()
            .map(|t| t.with_timezone(&Utc))
    }
    pub fn from_uuid(turn: &str) -> Option<Self> {
        // RFC 9562 §5.7: first 48 bits are Unix milliseconds. Not a message timestamp.
        if !crate::conversation::valid_thread(turn)
            || turn.as_bytes()[14] != b'7'
            || !b"89abAB".contains(&turn.as_bytes()[19])
        {
            return None;
        }
        let hex = turn[..13].replace('-', "");
        let at = DateTime::<Utc>::from_timestamp_millis(i64::from_str_radix(&hex, 16).ok()?)?;
        Some(Self {
            at: at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            basis: TimeBasis::UuidV7TurnStart,
        })
    }
}
pub fn source_time(e: &ConversationEvidence) -> Option<SourceTime> {
    e.source_time
        .clone()
        .or_else(|| SourceTime::from_uuid(&e.turn_id))
}
pub fn range(evidence: &[ConversationEvidence]) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let times: Option<Vec<_>> = evidence
        .iter()
        .map(|e| source_time(e)?.datetime())
        .collect();
    let times = times?;
    Some((*times.iter().min()?, *times.iter().max()?))
}
pub fn lesson_range(lesson: &Lesson) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    if !lesson.evidence.is_empty() {
        return None;
    }
    range(&lesson.conversation_evidence)
}
pub fn valid_key(key: &str) -> bool {
    (3..=120).contains(&key.len())
        && key
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-./:_".contains(&c))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChronologySummary {
    pub knowledge_key: Option<String>,
    pub source_from: Option<String>,
    pub source_to: Option<String>,
    pub source_time_precision: String,
    pub relation: String,
    pub related_lesson_ids: Vec<String>,
    pub related_truncated: bool,
    pub needs_reconciliation: bool,
    pub guidance: String,
}
pub fn summary(lesson: &Lesson, peers: &[Lesson], truncated: bool) -> Option<ChronologySummary> {
    if lesson.conversation_evidence.is_empty() && lesson.knowledge_key.is_none() {
        return None;
    }
    let own = lesson_range(lesson);
    let mut relation = if own.is_some() {
        "only_known"
    } else {
        "undated"
    };
    if !peers.is_empty() {
        relation = match own {
            None => "undated",
            Some((start, end)) => {
                let spans: Option<Vec<_>> = peers.iter().map(lesson_range).collect();
                match spans {
                    None => "undated",
                    Some(_)
                        if !precise(&lesson.conversation_evidence)
                            || peers.iter().any(|p| !precise(&p.conversation_evidence)) =>
                    {
                        "approximate"
                    }
                    Some(s) if s.iter().all(|(_, e)| *e < start) => "newer_context",
                    Some(s) if s.iter().any(|(a, _)| *a > end) => "older_context",
                    _ => "overlapping",
                }
            }
        };
    }
    Some(ChronologySummary {
        knowledge_key: lesson.knowledge_key.clone(),
        source_from: own.map(|(a, _)| a.to_rfc3339()),
        source_to: own.map(|(_, b)| b.to_rfc3339()),
        source_time_precision: if own.is_none() {
            "unknown"
        } else if precise(&lesson.conversation_evidence) {
            "message"
        } else {
            "turn_start_estimate"
        }
        .into(),
        relation: relation.into(),
        related_lesson_ids: peers.iter().take(8).map(|p| p.id.clone()).collect(),
        related_truncated: truncated || peers.len() > 8,
        needs_reconciliation: !peers.is_empty() || truncated || own.is_none(),
        guidance: concat!(
            "Source dates are not capture/storage dates or proof of correctness. ",
            "Read competing lessons and current docs/code; older_context is historical advice, ",
            "not a newer correction. Unknown/overlapping times cannot establish precedence."
        )
        .into(),
    })
}
pub fn guard_terminal(
    proof: &[ConversationEvidence],
    affected: &[Lesson],
) -> Result<(), &'static str> {
    if proof.is_empty() {
        return Ok(());
    }
    if !proof.iter().all(|e| {
        matches!(
            e.claim,
            ConversationClaim::Correction | ConversationClaim::ObservedOutcome
        )
    }) {
        return Err(
            "terminal chat changes require an explicit correction or inspected outcome; record a conflict for unresolved advice",
        );
    }
    if !precise(proof) {
        return Err(
            "turn-start estimates cannot establish exact precedence for a terminal correction; record a conflict or use current inspected PR evidence",
        );
    }
    let (earliest, _) = range(proof)
        .ok_or("undated chat evidence cannot retire or supersede knowledge; record a conflict")?;
    for lesson in affected {
        if !precise(&lesson.conversation_evidence) {
            return Err(
                "affected knowledge has unknown or approximate source dates; record a conflict or use current inspected PR evidence",
            );
        }
        let (_,latest)=lesson_range(lesson).ok_or("affected knowledge has unknown or mixed source dates; record a conflict or use inspected current PR evidence")?;
        if earliest <= latest {
            return Err(
                "older or equal-time chat evidence cannot retire or supersede newer knowledge; record a conflict",
            );
        }
    }
    Ok(())
}

fn precise(evidence: &[ConversationEvidence]) -> bool {
    !evidence.is_empty()
        && evidence
            .iter()
            .all(|e| source_time(e).is_some_and(|t| t.basis == TimeBasis::MessageTimestamp))
}
