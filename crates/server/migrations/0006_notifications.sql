-- In-app notifications (the bell). Only the newest few hundred are kept.
CREATE SEQUENCE notifications_seq;

CREATE TABLE notifications (
    id         TEXT PRIMARY KEY DEFAULT 'ntf-' || nextval('notifications_seq'),
    -- What raised it (camera_offline, person, …), for throttling and filters.
    kind       TEXT NOT NULL,
    camera_id  TEXT,
    level      TEXT NOT NULL,
    title      TEXT NOT NULL,
    message    TEXT NOT NULL,
    link       TEXT,
    read       BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX notifications_time ON notifications (created_at DESC);
