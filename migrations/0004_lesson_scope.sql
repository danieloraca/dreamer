CREATE TABLE lesson_scope (
    repository TEXT NOT NULL,
    lesson_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('component', 'path', 'exclude')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    value TEXT NOT NULL CHECK (length(value) > 0),
    PRIMARY KEY (repository, lesson_id, kind, ordinal),
    FOREIGN KEY (repository, lesson_id) REFERENCES lessons(repository, id)
);
CREATE INDEX lesson_scope_lookup ON lesson_scope(repository, kind, value, lesson_id);
