-- Watchgrid users (created only from the CLI) and browser sessions.
CREATE TABLE users (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    -- Argon2id PHC string.
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL,
    enabled       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_login    TIMESTAMPTZ
);

-- Only a hash of the cookie token is stored; `id` is a separate public
-- handle used to list and revoke sessions.
CREATE TABLE sessions (
    id         TEXT PRIMARY KEY,
    token_hash BYTEA NOT NULL UNIQUE,
    user_id    BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    client     TEXT NOT NULL,
    address    TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX sessions_user ON sessions (user_id);
