DROP INDEX IF EXISTS uniq_classroom_id_agent_id;

ALTER TABLE agent_session
    ADD COLUMN outdated bool DEFAULT false;

CREATE UNIQUE INDEX IF NOT EXISTS uniq_classroom_id_agent_id_outdated ON agent_session (classroom_id, agent_id, outdated);
