CREATE TABLE IF NOT EXISTS agent_session_history
(
    id           uuid      NOT NULL PRIMARY KEY,
    agent_id     agent_id  NOT NULL,
    classroom_id uuid      NOT NULL,
    lifetime     tstzrange NOT NULL
);
