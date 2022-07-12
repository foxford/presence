CREATE TABLE IF NOT EXISTS agent_session
(
    id           uuid    DEFAULT gen_random_uuid() NOT NULL PRIMARY KEY,
    agent_id     agent_id                          NOT NULL,
    classroom_id uuid                              NOT NULL,
    replica_id   text                              NOT NULL,
    started_at   timestamptz                       NOT NULL,
    outdated     boolean DEFAULT false
);

CREATE INDEX IF NOT EXISTS classroom_id ON agent_session (classroom_id);

CREATE UNIQUE INDEX IF NOT EXISTS uniq_classroom_id_agent_id_outdated ON agent_session (classroom_id, agent_id, outdated);
