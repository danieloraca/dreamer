CREATE TABLE lesson_knowledge_keys (
    repository TEXT NOT NULL,
    lesson_id TEXT NOT NULL,
    knowledge_key TEXT NOT NULL,
    PRIMARY KEY (repository, lesson_id),
    FOREIGN KEY (repository, lesson_id) REFERENCES lessons(repository, id)
);
CREATE INDEX knowledge_keys_topic ON lesson_knowledge_keys(repository,knowledge_key,lesson_id);
