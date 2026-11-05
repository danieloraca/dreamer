-- Additive: existing PR evidence and immutable lifecycle JSON remain unchanged.
CREATE TABLE conversation_evidence (
    repository TEXT NOT NULL,
    lesson_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    thread_id TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    PRIMARY KEY (repository, lesson_id, ordinal),
    FOREIGN KEY (repository, lesson_id) REFERENCES lessons(repository, id)
);
CREATE INDEX conversation_sources ON conversation_evidence(repository, thread_id);
