-- Dashboard password-login storage service (see plans/dashboard-auth-on-lit-payments.md).
--
-- Port of the former Cloudflare D1 schema (lit-dashboard-auth/migrations 0001 +
-- 0002). Timestamps are epoch seconds (BIGINT) so the conditional updates that
-- enforce the signup state machine port unchanged. Wallet addresses are
-- unverified client metadata and deliberately NOT unique: an attacker must not
-- be able to reserve a victim's public wallet and block their signup.
--
-- The service never stores passwords, API keys, or decryption keys. `verifier`
-- is a hash of an already Argon2id-stretched, domain-separated secret;
-- `envelope` is browser-encrypted ciphertext; `hash` columns are SHA-256 of
-- bearer tokens.

CREATE TABLE dashboard_auth_users (
    id         TEXT   PRIMARY KEY,
    email      TEXT   NOT NULL UNIQUE,
    salt       TEXT   NOT NULL,
    verifier   TEXT,
    version    BIGINT NOT NULL DEFAULT 1,
    state      TEXT   NOT NULL DEFAULT 'verified'
               CHECK (state IN ('verified', 'reserved', 'creating', 'active')),
    account    TEXT,
    envelope   TEXT,
    operation  TEXT,
    created_at BIGINT NOT NULL,
    CHECK ((state = 'active') = (envelope IS NOT NULL)),
    CHECK (state <> 'active' OR account IS NOT NULL)
);

CREATE TABLE dashboard_auth_sessions (
    hash       TEXT   PRIMARY KEY,
    user_id    TEXT   NOT NULL REFERENCES dashboard_auth_users (id),
    version    BIGINT NOT NULL,
    csrf       TEXT   NOT NULL,
    scope      TEXT   NOT NULL CHECK (scope IN ('signup', 'account')),
    expires_at BIGINT NOT NULL,
    idle_until BIGINT NOT NULL
);
CREATE INDEX dashboard_auth_sessions_user_idx ON dashboard_auth_sessions (user_id);
CREATE INDEX dashboard_auth_sessions_expiry_idx ON dashboard_auth_sessions (expires_at);

CREATE TABLE dashboard_auth_tokens (
    hash       TEXT   PRIMARY KEY,
    email      TEXT   NOT NULL,
    user_id    TEXT,
    version    BIGINT,
    purpose    TEXT   NOT NULL CHECK (purpose IN ('signup', 'email')),
    expires_at BIGINT NOT NULL,
    claim      TEXT
);
CREATE INDEX dashboard_auth_tokens_expiry_idx ON dashboard_auth_tokens (expires_at);

-- Encrypted pending email, retried by the outbox job with Resend idempotency keys.
CREATE TABLE dashboard_auth_outbox (
    id         TEXT   PRIMARY KEY,
    payload    TEXT   NOT NULL,
    attempts   BIGINT NOT NULL DEFAULT 0,
    next_at    BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);
CREATE INDEX dashboard_auth_outbox_due_idx ON dashboard_auth_outbox (next_at);

-- Conditional counters for per-IP and per-identity rate limits.
CREATE TABLE dashboard_auth_limits (
    key        TEXT   PRIMARY KEY,
    count      BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);
