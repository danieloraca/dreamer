use super::{LessonStore, SearchQuery, WriteOutcome};
use crate::lesson::{
    Evidence, ExtractionMethod, Lesson, LessonStatus, Provenance, PullRequestRef, StoredLesson,
    validate_repository,
};
use crate::lifecycle::{
    ChangeAction, HistoryPage, HistoryQuery, LessonChange, LifecycleSummary, StoredChange,
};
use crate::retrieval::{self, LessonScope, RecallPage, RecallQuery};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::{error::Error, fmt, path::Path, time::Duration};

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "knowledge",
        sql: include_str!("../../migrations/0001_knowledge.sql"),
    },
    Migration {
        version: 2,
        name: "lookup_indexes",
        sql: include_str!("../../migrations/0002_lookup_indexes.sql"),
    },
    Migration {
        version: 3,
        name: "lesson_lifecycle",
        sql: include_str!("../../migrations/0003_lesson_lifecycle.sql"),
    },
    Migration {
        version: 4,
        name: "lesson_scope",
        sql: include_str!("../../migrations/0004_lesson_scope.sql"),
    },
];

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    Invalid(&'static str),
    Schema(String),
    Conflict(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "SQLite: {error}"),
            Self::Invalid(message) => write!(f, "invalid knowledge: {message}"),
            Self::Schema(message) => write!(f, "incompatible database schema: {message}"),
            Self::Conflict(message) => write!(f, "knowledge conflict: {message}"),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sqlite(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

pub struct SqliteStore {
    connection: Connection,
}

impl SqliteStore {
    /// Creates/migrates a database. The caller owns directory creation.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        // Reject incompatible histories before changing journal mode. Recheck
        // inside the migration transaction to handle concurrent initialization.
        let has_ledger: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations')",
            [], |row| row.get(0),
        )?;
        if has_ledger {
            check_schema(&connection, MIGRATIONS, false)?;
        }
        // Establish WAL before the write transaction so its shared-memory files
        // exist when another connection opens read-only, even before import #1.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&mut connection, MIGRATIONS)?;
        Ok(Self { connection })
    }

    /// Reads an existing, current schema; does not create or migrate a database.
    pub fn open_read_only(path: &Path) -> Result<Self, StoreError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        check_schema(&connection, MIGRATIONS, true)?;
        Ok(Self { connection })
    }

    /// Writes an existing, current database without creating or migrating it.
    pub fn open_existing(path: &Path) -> Result<Self, StoreError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        check_schema(&connection, MIGRATIONS, true)?;
        Ok(Self { connection })
    }

    pub fn schema_version(&self) -> Result<i64, StoreError> {
        Ok(self
            .connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })?)
    }
}

fn check_schema(
    connection: &Connection,
    migrations: &[Migration],
    require_current: bool,
) -> Result<usize, StoreError> {
    let mut statement =
        connection.prepare("SELECT version, name FROM schema_migrations ORDER BY version")?;
    let installed = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if installed.len() > migrations.len() {
        return Err(StoreError::Schema(
            "database is newer than this binary".into(),
        ));
    }
    for (index, (version, name)) in installed.iter().enumerate() {
        let expected = &migrations[index];
        if *version != expected.version || name != expected.name {
            return Err(StoreError::Schema(
                "unknown, renamed or non-contiguous migrations".into(),
            ));
        }
    }
    if require_current && installed.len() != migrations.len() {
        return Err(StoreError::Schema(
            "run `dreamer db init` to apply migrations".into(),
        ));
    }
    Ok(installed.len())
}

fn migrate(connection: &mut Connection, migrations: &[Migration]) -> Result<(), StoreError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )?;
    let applied = check_schema(&transaction, migrations, false)?;
    for migration in &migrations[applied..] {
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            params![migration.version, migration.name],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn read_lesson(
    connection: &Connection,
    repository: &str,
    id: &str,
) -> Result<Option<StoredLesson>, StoreError> {
    let result = connection
        .query_row(
            "SELECT summary, applies_when, status, producer, method, stored_at
         FROM lessons WHERE repository = ?1 AND id = ?2",
            params![repository, id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((summary, applies_when, status, producer, method, stored_at)) = result else {
        return Ok(None);
    };
    let status = match status.as_str() {
        "candidate" => LessonStatus::Candidate,
        "verified" => LessonStatus::Verified,
        "deprecated" => LessonStatus::Deprecated,
        _ => return Err(StoreError::Schema("unknown lesson status".into())),
    };
    let method = match method.as_str() {
        "manual" => ExtractionMethod::Manual,
        "fixture" => ExtractionMethod::Fixture,
        _ => return Err(StoreError::Schema("unknown extraction method".into())),
    };
    let mut statement = connection.prepare(
        "SELECT e.source_url, e.commit_sha, e.description, p.number, p.head_sha, p.url, p.title
         FROM lesson_evidence e JOIN pull_requests p
           ON p.repository = e.repository AND p.number = e.pr_number AND p.head_sha = e.pr_head_sha
         WHERE e.repository = ?1 AND e.lesson_id = ?2 ORDER BY e.ordinal",
    )?;
    let evidence = statement
        .query_map(params![repository, id], |row| {
            Ok(Evidence {
                source_url: row.get(0)?,
                commit_sha: row.get(1)?,
                description: row.get(2)?,
                pull_request: PullRequestRef {
                    number: row.get(3)?,
                    head_sha: row.get(4)?,
                    url: row.get(5)?,
                    title: row.get(6)?,
                },
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(StoredLesson {
        lesson: Lesson {
            id: id.into(),
            repository: repository.into(),
            summary,
            applies_when,
            status,
            scope: read_scope(connection, repository, id)?,
            provenance: Provenance { producer, method },
            evidence,
        },
        stored_at,
        lifecycle: read_lifecycle(connection, repository, id)?,
    }))
}

fn read_scope(
    connection: &Connection,
    repository: &str,
    id: &str,
) -> Result<LessonScope, StoreError> {
    let mut statement=connection.prepare("SELECT kind,value FROM lesson_scope WHERE repository=?1 AND lesson_id=?2 ORDER BY kind,ordinal")?;
    let rows = statement
        .query_map(params![repository, id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut scope = LessonScope::default();
    for (kind, value) in rows {
        match kind.as_str() {
            "component" => scope.components.push(value),
            "path" => scope.paths.push(value),
            "exclude" => scope.excluded_terms.push(value),
            _ => return Err(StoreError::Schema("unknown lesson scope kind".into())),
        }
    }
    Ok(scope)
}
impl SqliteStore {
    /// Parameterized candidate selection followed by the shared deterministic ranker.
    pub fn recall(&self, query: &RecallQuery) -> Result<RecallPage, StoreError> {
        query.validate().map_err(StoreError::Invalid)?;
        let mut predicates = Vec::new();
        let mut values = vec![query.repository.clone()];
        for term in &query.queries {
            values.push(term.trim().into());
            let parameter = format!("?{}", values.len());
            predicates.push(format!("(instr(lower(l.summary || ' ' || l.applies_when),lower({parameter}))>0 OR EXISTS(SELECT 1 FROM lesson_scope s WHERE s.repository=l.repository AND s.lesson_id=l.id AND s.kind!='exclude' AND instr(lower(s.value),lower({parameter}))>0))"));
        }
        for component in &query.components {
            values.push(component.trim().into());
            let parameter = format!("?{}", values.len());
            predicates.push(format!("EXISTS(SELECT 1 FROM lesson_scope s WHERE s.repository=l.repository AND s.lesson_id=l.id AND s.kind='component' AND lower(s.value)=lower({parameter}))"));
        }
        for path in &query.paths {
            values.push(path.clone());
            let parameter = format!("?{}", values.len());
            predicates.push(format!("EXISTS(SELECT 1 FROM lesson_scope s WHERE s.repository=l.repository AND s.lesson_id=l.id AND s.kind='path' AND (s.value={parameter} OR (substr(s.value,-1)='/' AND substr({parameter},1,length(s.value))=s.value)))"));
        }
        let tx = self.connection.unchecked_transaction()?;
        let mut statement=tx.prepare(&format!("SELECT l.id FROM lessons l WHERE l.repository=?1 AND l.status!='deprecated' AND ({}) ORDER BY l.stored_at DESC,l.id ASC LIMIT 1001",predicates.join(" OR ")))?;
        let ids = statement
            .query_map(rusqlite::params_from_iter(values), |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let candidate_limit_reached = ids.len() > 1000;
        let lessons = ids
            .iter()
            .take(1000)
            .map(|id| {
                read_lesson(&tx, &query.repository, id)?
                    .ok_or_else(|| StoreError::Schema("recall candidate disappeared".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        tx.commit()?;
        retrieval::rank(query, lessons, candidate_limit_reached).map_err(StoreError::Invalid)
    }
}

fn read_lifecycle(
    connection: &Connection,
    repository: &str,
    id: &str,
) -> Result<Option<LifecycleSummary>, StoreError> {
    let terminal: Option<(Option<String>, String)> = connection.query_row(
        "SELECT target_id, reason FROM lesson_changes WHERE repository=?1 AND source_id=?2 AND action IN ('supersede','retire')",
        params![repository,id], |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    let mut statement = connection.prepare(
        "SELECT DISTINCT CASE WHEN c.source_id=?2 THEN c.target_id ELSE c.source_id END AS other
         FROM lesson_changes c JOIN lessons a ON a.repository=c.repository AND a.id=c.source_id
         JOIN lessons b ON b.repository=c.repository AND b.id=c.target_id
         WHERE c.repository=?1 AND c.action='conflict' AND (c.source_id=?2 OR c.target_id=?2)
         AND a.status!='deprecated' AND b.status!='deprecated' ORDER BY other LIMIT 11",
    )?;
    let mut conflicts = statement
        .query_map(params![repository, id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let conflicts_truncated = conflicts.len() > 10;
    conflicts.truncate(10);
    if terminal.is_none() && conflicts.is_empty() {
        return Ok(None);
    }
    let (superseded_by, retirement_reason) = match terminal {
        Some((target, reason)) => (target, Some(reason)),
        None => (None, None),
    };
    Ok(Some(LifecycleSummary {
        superseded_by,
        retirement_reason,
        conflicts,
        conflicts_truncated,
    }))
}
fn stored_change(
    id: String,
    payload: String,
    stored_at: String,
) -> Result<StoredChange, StoreError> {
    let change = serde_json::from_str(&payload)
        .map_err(|_| StoreError::Schema("invalid lifecycle payload".into()))?;
    Ok(StoredChange {
        id,
        change,
        stored_at,
    })
}
impl SqliteStore {
    /// Append evidence and project a terminal status in one write transaction.
    pub fn change_lesson(
        &mut self,
        proposed: &LessonChange,
    ) -> Result<(WriteOutcome, StoredChange), StoreError> {
        proposed.validate().map_err(StoreError::Invalid)?;
        let change = proposed.canonical();
        let id = change.id();
        let payload = serde_json::to_string(&change).expect("change serialization");
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT payload,stored_at FROM lesson_changes WHERE repository=?1 AND id=?2",
                params![change.repository, id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((old, time)) = existing {
            if old != payload {
                return Err(StoreError::Conflict("lifecycle identity conflict".into()));
            }
            return Ok((WriteOutcome::Unchanged, stored_change(id, old, time)?));
        }
        let source = read_lesson(&tx, &change.repository, &change.source_id)?
            .ok_or(StoreError::Invalid("source lesson not found in repository"))?;
        if source.lesson.status == LessonStatus::Deprecated {
            return Err(StoreError::Conflict(
                "source lesson is already retired or superseded; inspect its history".into(),
            ));
        }
        if let Some(target) = &change.target_id {
            let target = read_lesson(&tx, &change.repository, target)?
                .ok_or(StoreError::Invalid("target lesson not found in repository"))?;
            if target.lesson.status == LessonStatus::Deprecated {
                return Err(StoreError::Conflict(
                    "target lesson is retired or superseded; select its active successor".into(),
                ));
            }
        }
        tx.execute("INSERT INTO lesson_changes (repository,id,source_id,action,target_id,reason,payload) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![change.repository,id,change.source_id,change.action.as_str(),change.target_id,change.reason,payload])?;
        if change.action != ChangeAction::Conflict {
            tx.execute(
                "UPDATE lessons SET status='deprecated' WHERE repository=?1 AND id=?2",
                params![change.repository, change.source_id],
            )?;
        }
        let time = tx.query_row(
            "SELECT stored_at FROM lesson_changes WHERE repository=?1 AND id=?2",
            params![change.repository, id],
            |row| row.get(0),
        )?;
        tx.commit()?;
        Ok((
            WriteOutcome::Inserted,
            StoredChange {
                id,
                change,
                stored_at: time,
            },
        ))
    }
    pub fn lesson_history(&self, query: &HistoryQuery) -> Result<HistoryPage, StoreError> {
        validate_repository(&query.repository).map_err(StoreError::Invalid)?;
        if query.id.trim().is_empty()
            || query.id.len() > 1000
            || !(1..=50).contains(&query.limit)
            || query.offset > 1_000_000
            || query.change_id.as_ref().is_some_and(|id| id.len() > 100)
        {
            return Err(StoreError::Invalid(
                "invalid history identity or pagination",
            ));
        }
        let mut statement=self.connection.prepare(
            "SELECT id,payload,stored_at FROM lesson_changes WHERE repository=?1 AND (source_id=?2 OR target_id=?2)
             AND (?3 IS NULL OR id=?3) ORDER BY stored_at DESC,id ASC LIMIT ?4 OFFSET ?5")?;
        let rows = statement
            .query_map(
                params![
                    query.repository,
                    query.id,
                    query.change_id,
                    (query.limit + 1) as i64,
                    query.offset as i64
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let next_offset = if rows.len() > query.limit {
            Some(query.offset + query.limit)
        } else {
            None
        };
        let changes = rows
            .into_iter()
            .take(query.limit)
            .map(|(id, payload, time)| stored_change(id, payload, time))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HistoryPage {
            repository: query.repository.clone(),
            id: query.id.clone(),
            changes,
            next_offset,
        })
    }
}

impl SqliteStore {
    /// Scope every aggregate/read to one repository, including deprecated records.
    /// A deferred read transaction keeps counts and the page on one SQLite snapshot.
    pub fn browse(
        &mut self,
        query: &super::BrowseQuery,
    ) -> Result<super::KnowledgePage, StoreError> {
        validate_repository(&query.repository).map_err(StoreError::Invalid)?;
        if !(1..=50).contains(&query.limit)
            || query.offset > 1_000_000
            || query.text.chars().count() > 500
            || query
                .status
                .as_deref()
                .is_some_and(|status| !matches!(status, "candidate" | "verified" | "deprecated"))
        {
            return Err(StoreError::Invalid("invalid browse filter or pagination"));
        }
        let tx = self.connection.transaction()?;
        let counts = tx.query_row(
            "SELECT COUNT(*), COALESCE(SUM(status = 'candidate'), 0),
                    COALESCE(SUM(status = 'verified'), 0), COALESCE(SUM(status = 'deprecated'), 0),
                    (SELECT COUNT(DISTINCT number) FROM pull_requests WHERE repository = ?1), MAX(stored_at)
             FROM lessons WHERE repository = ?1",
            params![query.repository],
            |row| Ok(super::KnowledgeCounts { total: row.get(0)?, candidate: row.get(1)?,
                verified: row.get(2)?, deprecated: row.get(3)?, source_prs: row.get(4)?, last_stored_at: row.get(5)? }),
        )?;
        let predicate = "repository = ?1 AND (?2 IS NULL OR status = ?2)
            AND instr(lower(id || ' ' || summary || ' ' || applies_when), lower(?3)) > 0";
        let matching = tx.query_row(
            &format!("SELECT COUNT(*) FROM lessons WHERE {predicate}"),
            params![query.repository, query.status, query.text.trim()],
            |row| row.get(0),
        )?;
        let ids = {
            let mut statement = tx.prepare(&format!(
                "SELECT id FROM lessons WHERE {predicate} ORDER BY stored_at DESC, id ASC LIMIT ?4 OFFSET ?5"))?;
            statement
                .query_map(
                    params![
                        query.repository,
                        query.status,
                        query.text.trim(),
                        query.limit as i64,
                        query.offset as i64
                    ],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?
        };
        let lessons = ids
            .iter()
            .map(|id| {
                read_lesson(&tx, &query.repository, id)?
                    .ok_or_else(|| StoreError::Schema("lesson disappeared during browse".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit()?;
        Ok(super::KnowledgePage {
            repository: query.repository.clone(),
            counts,
            matching,
            limit: query.limit,
            offset: query.offset,
            lessons,
        })
    }
}

impl LessonStore for SqliteStore {
    type Error = StoreError;

    /// Atomic, insert-only import. Exact retries are no-ops; conflicting IDs fail.
    fn put_candidate(&mut self, lesson: &Lesson) -> Result<WriteOutcome, Self::Error> {
        lesson.validate_candidate().map_err(StoreError::Invalid)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = read_lesson(&transaction, &lesson.repository, &lesson.id)? {
            if existing.lesson == *lesson {
                return Ok(WriteOutcome::Unchanged);
            }
            return Err(StoreError::Conflict(format!(
                "lesson {}/{} already exists with different content",
                lesson.repository, lesson.id
            )));
        }
        let method = match lesson.provenance.method {
            ExtractionMethod::Manual => "manual",
            ExtractionMethod::Fixture => "fixture",
        };
        transaction.execute(
            "INSERT INTO lessons (repository, id, summary, applies_when, status, producer, method)
             VALUES (?1, ?2, ?3, ?4, 'candidate', ?5, ?6)",
            params![
                lesson.repository,
                lesson.id,
                lesson.summary,
                lesson.applies_when,
                lesson.provenance.producer,
                method
            ],
        )?;
        for (ordinal, evidence) in lesson.evidence.iter().enumerate() {
            let pr = &evidence.pull_request;
            transaction.execute(
                "INSERT INTO pull_requests (repository, number, head_sha, url, title) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (repository, number, head_sha) DO NOTHING",
                params![lesson.repository, pr.number, pr.head_sha, pr.url, pr.title],
            )?;
            let (url, title): (String, String) = transaction.query_row(
                "SELECT url, title FROM pull_requests WHERE repository = ?1 AND number = ?2 AND head_sha = ?3",
                params![lesson.repository, pr.number, pr.head_sha], |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if url != pr.url || title != pr.title {
                return Err(StoreError::Conflict(format!(
                    "PR #{} revision {} has different source metadata",
                    pr.number, pr.head_sha
                )));
            }
            transaction.execute(
                "INSERT INTO lesson_evidence (repository, lesson_id, ordinal, pr_number, pr_head_sha, source_url, commit_sha, description)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![lesson.repository, lesson.id, ordinal as i64, pr.number, pr.head_sha, evidence.source_url, evidence.commit_sha, evidence.description],
            )?;
        }
        for (kind, values) in [
            ("component", &lesson.scope.components),
            ("path", &lesson.scope.paths),
            ("exclude", &lesson.scope.excluded_terms),
        ] {
            for (ordinal, value) in values.iter().enumerate() {
                transaction.execute("INSERT INTO lesson_scope(repository,lesson_id,kind,ordinal,value) VALUES (?1,?2,?3,?4,?5)",params![lesson.repository,lesson.id,kind,ordinal as i64,value])?;
            }
        }
        transaction.commit()?;
        Ok(WriteOutcome::Inserted)
    }

    fn get(&self, repository: &str, id: &str) -> Result<Option<StoredLesson>, Self::Error> {
        validate_repository(repository).map_err(StoreError::Invalid)?;
        if id.trim().is_empty() {
            return Err(StoreError::Invalid("lesson ID must not be empty"));
        }
        let tx = self.connection.unchecked_transaction()?;
        let lesson = read_lesson(&tx, repository, id)?;
        tx.commit()?;
        Ok(lesson)
    }

    fn search(&self, query: &SearchQuery) -> Result<Vec<StoredLesson>, Self::Error> {
        validate_repository(&query.repository).map_err(StoreError::Invalid)?;
        if query.text.trim().is_empty() || !(1..=100).contains(&query.limit) {
            return Err(StoreError::Invalid(
                "search needs nonempty text and a limit between 1 and 100",
            ));
        }
        // Status, evidence and lifecycle belong to the same read snapshot while
        // concurrent agents can retire or supersede a lesson.
        let tx = self.connection.unchecked_transaction()?;
        // Literal substring matching: %, _ and quotes are data, not SQL syntax.
        let mut statement = tx.prepare(
            "SELECT id FROM lessons WHERE repository = ?1 AND status != 'deprecated'
             AND instr(lower(summary || ' ' || applies_when), lower(?2)) > 0
             ORDER BY stored_at DESC, id ASC LIMIT ?3",
        )?;
        let ids = statement
            .query_map(
                params![query.repository, query.text.trim(), query.limit as i64],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let lessons = ids
            .into_iter()
            .map(|id| {
                read_lesson(&tx, &query.repository, &id)?
                    .ok_or_else(|| StoreError::Schema("lesson disappeared during search".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        tx.commit()?;
        Ok(lessons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_migration_rolls_back_schema_and_ledger() {
        let mut connection = Connection::open_in_memory().unwrap();
        let migrations = [Migration {
            version: 1,
            name: "broken",
            sql: "CREATE TABLE partial (id INTEGER); INVALID SQL;",
        }];
        assert!(migrate(&mut connection, &migrations).is_err());
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('partial', 'schema_migrations')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
