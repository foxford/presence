DROP INDEX uniq_classroom_id_agent_id_outdated;

ALTER TABLE agent_session
    DROP COLUMN outdated;

CREATE UNIQUE INDEX IF NOT EXISTS uniq_classroom_id_agent_id
    ON agent_session (classroom_id, agent_id);
