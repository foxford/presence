ALTER TABLE agent_session
    ADD COLUMN replica_id text NOT NULL,
    DROP replica_id,
    DROP CONSTRAINT IF EXISTS agent_session_replica_id_fk;
