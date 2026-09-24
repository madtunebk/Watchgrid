-- Server-wide settings as JSON documents, one row per section
-- (e.g. "retention"). Small, rarely written.
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
