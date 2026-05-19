-- Operators: the allowlist of email addresses permitted to use the
-- admin portal. Roles:
--   'mod'   — can grant credits up to per-grant/daily caps
--   'admin' — everything (manage operators, override LITKEY rate, etc.)

CREATE TABLE operators (
    id           BIGSERIAL PRIMARY KEY,
    email        TEXT      NOT NULL UNIQUE,
    role         TEXT      NOT NULL CHECK (role IN ('mod', 'admin')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_login_at TIMESTAMPTZ
);

-- Case-insensitive email lookup. Storing as lowercase is the convention
-- this codebase enforces at write time, but the index lets us match
-- variants defensively.
CREATE UNIQUE INDEX operators_email_lower_idx ON operators (lower(email));

-- Sessions: opaque cookie tokens issued after magic-link verification.
-- Token is random 32-byte base64url string. We store the raw token because
-- only the holder of the cookie can use it; no enumeration risk if a
-- read of this table happens to leak (token alone authenticates).

CREATE TABLE sessions (
    token        TEXT      PRIMARY KEY,
    operator_id  BIGINT    NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX sessions_operator_id_idx ON sessions (operator_id);
CREATE INDEX sessions_expires_at_idx  ON sessions (expires_at);
