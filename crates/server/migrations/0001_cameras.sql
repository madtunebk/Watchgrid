-- Cameras: configuration only. Live state (online, recording, motion,
-- stream details) is held in memory by the server, never stored here.
CREATE TABLE cameras (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    description         TEXT NOT NULL DEFAULT '',
    location            TEXT NOT NULL DEFAULT '',
    enabled             BOOLEAN NOT NULL DEFAULT TRUE,
    host                TEXT NOT NULL,
    username            TEXT NOT NULL DEFAULT '',
    -- AES-256-GCM ciphertext from the credential store; never plaintext.
    password_enc        BYTEA,
    main_stream_url     TEXT NOT NULL,
    sub_stream_url      TEXT,
    -- {"url": ..., "username": ...}; the ONVIF password is stored encrypted below.
    onvif               JSONB,
    onvif_password_enc  BYTEA,
    recording           JSONB NOT NULL,
    motion              JSONB NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Names are unique regardless of case.
CREATE UNIQUE INDEX cameras_name_unique ON cameras (lower(name));
