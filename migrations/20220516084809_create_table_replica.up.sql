CREATE TABLE IF NOT EXISTS replica
(
    id            uuid        DEFAULT gen_random_uuid() NOT NULL PRIMARY KEY,
    label         text                                  NOT NULL,
    ip            inet                                  NOT NULL,
    registered_at timestamptz DEFAULT NOW()             NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uniq_replica_label ON replica (label);
