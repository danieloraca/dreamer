CREATE TABLE learning_runs (
    repository TEXT NOT NULL,
    id TEXT NOT NULL,
    phase TEXT NOT NULL CHECK (phase IN ('capture','publication','reconciliation')),
    state TEXT NOT NULL CHECK (state IN ('started','succeeded','failed','skipped')),
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    first_reported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    reported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (repository,id)
);
CREATE INDEX learning_runs_repository_time ON learning_runs(repository,reported_at DESC,id);
