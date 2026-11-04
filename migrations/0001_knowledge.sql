CREATE TABLE pull_requests (
    repository TEXT NOT NULL,
    number INTEGER NOT NULL CHECK (number > 0),
    head_sha TEXT NOT NULL,
    url TEXT NOT NULL,
    title TEXT NOT NULL,
    PRIMARY KEY (repository, number, head_sha)
);

CREATE TABLE lessons (
    repository TEXT NOT NULL,
    id TEXT NOT NULL,
    summary TEXT NOT NULL,
    applies_when TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('candidate', 'verified', 'deprecated')),
    producer TEXT NOT NULL,
    method TEXT NOT NULL CHECK (method IN ('manual', 'fixture')),
    stored_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (repository, id)
);

CREATE TABLE lesson_evidence (
    repository TEXT NOT NULL,
    lesson_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    pr_number INTEGER NOT NULL,
    pr_head_sha TEXT NOT NULL,
    source_url TEXT NOT NULL,
    commit_sha TEXT,
    description TEXT NOT NULL,
    PRIMARY KEY (repository, lesson_id, ordinal),
    FOREIGN KEY (repository, lesson_id) REFERENCES lessons (repository, id),
    FOREIGN KEY (repository, pr_number, pr_head_sha)
        REFERENCES pull_requests (repository, number, head_sha)
);
