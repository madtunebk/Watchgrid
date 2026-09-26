-- Finalized recordings. Rows are only inserted once the file on disk is
-- complete and playable; files being written never appear here.
-- No foreign key: recordings outlive a deleted camera.
CREATE TABLE recordings (
    id          TEXT PRIMARY KEY,
    camera_id   TEXT NOT NULL,
    reason      TEXT NOT NULL,
    start_time  TIMESTAMPTZ NOT NULL,
    end_time    TIMESTAMPTZ NOT NULL,
    duration_ms BIGINT NOT NULL,
    file_size   BIGINT NOT NULL,
    -- Relative to the recordings directory.
    path        TEXT NOT NULL,
    codec       TEXT NOT NULL,
    width       INTEGER NOT NULL,
    height      INTEGER NOT NULL,
    protected   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX recordings_camera_time ON recordings (camera_id, start_time);
CREATE INDEX recordings_time ON recordings (start_time);
