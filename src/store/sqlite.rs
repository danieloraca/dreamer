use super::{LessonStore, SearchQuery, WriteOutcome};
use crate::lesson::{
    Evidence, ExtractionMethod, Lesson, LessonStatus, Provenance, PullRequestRef, StoredLesson,
    validate_repository,
};
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
            provenance: Provenance { producer, method },
            evidence,
        },
        stored_at,
    }))
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
        transaction.commit()?;
        Ok(WriteOutcome::Inserted)
    }

    fn get(&self, repository: &str, id: &str) -> Result<Option<StoredLesson>, Self::Error> {
        validate_repository(repository).map_err(StoreError::Invalid)?;
        if id.trim().is_empty() {
            return Err(StoreError::Invalid("lesson ID must not be empty"));
        }
        read_lesson(&self.connection, repository, id)
    }

    fn search(&self, query: &SearchQuery) -> Result<Vec<StoredLesson>, Self::Error> {
        validate_repository(&query.repository).map_err(StoreError::Invalid)?;
        if query.text.trim().is_empty() || !(1..=100).contains(&query.limit) {
            return Err(StoreError::Invalid(
                "search needs nonempty text and a limit between 1 and 100",
            ));
        }
        // Literal substring matching: %, _ and quotes are data, not SQL syntax.
        let mut statement = self.connection.prepare(
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
        ids.into_iter()
            .map(|id| {
                read_lesson(&self.connection, &query.repository, &id)?
                    .ok_or_else(|| StoreError::Schema("lesson disappeared during search".into()))
            })
            .collect()
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
