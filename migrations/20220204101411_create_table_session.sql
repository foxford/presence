CREATE TABLE IF NOT EXISTS session
(
    uuid         uuid,
    agent_id     agent_id,
    classroom_id uuid,
    replica_id   text,
    started_at   timestamptz
);
