CREATE TABLE IF NOT EXISTS session
(
    uuid         uuid DEFAULT gen_random_uuid() NOT NULL,
    agent_id     agent_id                       NOT NULL,
    classroom_id uuid                           NOT NULL,
    replica_id   text                           NOT NULL,
    started_at   timestamptz                    NOT NULL
    );
