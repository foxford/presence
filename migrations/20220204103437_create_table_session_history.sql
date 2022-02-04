CREATE TABLE IF NOT EXISTS session_history
(
    uuid         uuid,
    agent_id     agent_id,
    classroom_id uuid,
    lifetime     tstzrange
);
