CREATE TABLE IF NOT EXISTS old_agent_session
(
    id           uuid        NOT NULL PRIMARY KEY,
    agent_id     agent_id    NOT NULL,
    classroom_id uuid        NOT NULL,
    replica_id   text        NOT NULL,
    started_at   timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS classroom_id ON old_agent_session (replica_id);
