ALTER TABLE agent_session
    DROP COLUMN replica_id,
    ADD replica_id uuid NOT NULL,
    ADD CONSTRAINT agent_session_replica_id_fk
        FOREIGN KEY (replica_id) REFERENCES replica;

