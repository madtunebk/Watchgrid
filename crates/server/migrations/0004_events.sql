-- Durable events: meaningful transitions only (never per-frame state).
-- An event with no end_time is still in progress (an outage, a recording).
CREATE SEQUENCE events_seq;

CREATE TABLE events (
    id           TEXT PRIMARY KEY DEFAULT 'evt-' || nextval('events_seq'),
    camera_id    TEXT NOT NULL,
    kind         TEXT NOT NULL,
    start_time   TIMESTAMPTZ NOT NULL,
    end_time     TIMESTAMPTZ,
    recording_id TEXT,
    -- Human-readable cause, e.g. "Supervisor: connection refused".
    source       TEXT NOT NULL,
    -- Which integration raised it: watchgrid, later onvif, ezviz, hikvision…
    origin       TEXT NOT NULL DEFAULT 'watchgrid',
    protected    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX events_start ON events (start_time DESC, id DESC);
CREATE INDEX events_camera_start ON events (camera_id, start_time DESC);
-- At most one open event of each kind per camera (makes writes idempotent).
CREATE UNIQUE INDEX events_one_open ON events (camera_id, kind) WHERE end_time IS NULL;
