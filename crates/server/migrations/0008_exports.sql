-- Export destinations (cloud / NAS targets) and upload jobs.
CREATE SEQUENCE export_targets_seq;
CREATE TABLE export_targets (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    endpoint    TEXT NOT NULL,
    location    TEXT NOT NULL,
    username    TEXT NOT NULL,
    -- Encrypted with the credential store (AAD "export:<id>:secret").
    secret_enc  BYTEA,
    auto_upload TEXT NOT NULL,
    -- Last error that makes the target unusable (bad credentials…).
    problem     TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE SEQUENCE export_jobs_seq;
CREATE TABLE export_jobs (
    id           TEXT PRIMARY KEY DEFAULT 'job-' || nextval('export_jobs_seq'),
    event_id     TEXT NOT NULL,
    recording_id TEXT NOT NULL,
    target_id    TEXT NOT NULL REFERENCES export_targets (id) ON DELETE CASCADE,
    state        TEXT NOT NULL,
    bytes_total  BIGINT NOT NULL DEFAULT 0,
    bytes_done   BIGINT NOT NULL DEFAULT 0,
    link         TEXT,
    message      TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX export_jobs_state ON export_jobs (state);
CREATE INDEX export_jobs_recording ON export_jobs (recording_id, target_id);
