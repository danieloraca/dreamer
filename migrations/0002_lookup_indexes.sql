CREATE INDEX lessons_repository_status_time
    ON lessons (repository, status, stored_at DESC, id);
CREATE INDEX evidence_pull_request
    ON lesson_evidence (repository, pr_number, pr_head_sha);
