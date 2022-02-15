CREATE TABLE IF NOT EXISTS session_history
(
    uuid         uuid DEFAULT gen_random_uuid() NOT NULL,
    agent_id     agent_id                       NOT NULL,
    classroom_id uuid                           NOT NULL,
    lifetime     tstzrange                      NOT NULL
    );
