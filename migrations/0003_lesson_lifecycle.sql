CREATE TABLE lesson_changes (
    repository TEXT NOT NULL,
    id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('supersede', 'retire', 'conflict')),
    target_id TEXT,
    reason TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    stored_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (repository, id),
    FOREIGN KEY (repository, source_id) REFERENCES lessons(repository, id),
    FOREIGN KEY (repository, target_id) REFERENCES lessons(repository, id),
    CHECK ((action = 'retire' AND target_id IS NULL) OR
           (action IN ('supersede', 'conflict') AND target_id IS NOT NULL AND source_id != target_id))
);
CREATE UNIQUE INDEX lesson_changes_terminal_source
    ON lesson_changes(repository, source_id) WHERE action IN ('supersede', 'retire');
CREATE INDEX lesson_changes_source ON lesson_changes(repository, source_id, stored_at DESC, id);
CREATE INDEX lesson_changes_target ON lesson_changes(repository, target_id, stored_at DESC, id);
