PRAGMA foreign_keys = ON;
CREATE TABLE auth_users (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  salt TEXT NOT NULL,
  verifier TEXT,
  version INTEGER NOT NULL DEFAULT 1,
  state TEXT NOT NULL DEFAULT 'verified' CHECK(state IN ('verified','reserved','creating','active')),
  account TEXT UNIQUE,
  envelope TEXT,
  operation TEXT,
  created_at INTEGER NOT NULL,
  CHECK ((state = 'active') = (envelope IS NOT NULL)),
  CHECK (state != 'active' OR account IS NOT NULL)
);
CREATE TABLE auth_sessions (
  hash TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES auth_users(id),
  version INTEGER NOT NULL,
  csrf TEXT NOT NULL,
  scope TEXT NOT NULL CHECK(scope IN ('signup','account')),
  expires_at INTEGER NOT NULL,
  idle_until INTEGER NOT NULL
);
CREATE INDEX sessions_user ON auth_sessions(user_id);
CREATE INDEX sessions_expiry ON auth_sessions(expires_at);
CREATE TABLE auth_tokens (
  hash TEXT PRIMARY KEY,
  email TEXT NOT NULL,
  user_id TEXT,
  version INTEGER,
  purpose TEXT NOT NULL CHECK(purpose IN ('signup','email')),
  expires_at INTEGER NOT NULL,
  claim TEXT
);
CREATE INDEX tokens_expiry ON auth_tokens(expires_at);
CREATE TABLE auth_outbox (
  id TEXT PRIMARY KEY,
  payload TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);
CREATE INDEX outbox_due ON auth_outbox(next_at);
CREATE TABLE auth_limits (
  key TEXT PRIMARY KEY,
  count INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);
