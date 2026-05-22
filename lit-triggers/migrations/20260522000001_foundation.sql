CREATE TABLE users (
  id UUID PRIMARY KEY,
  email TEXT UNIQUE NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_login_at TIMESTAMPTZ
);

CREATE TABLE sessions (
  token_hash TEXT PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX sessions_user_id_idx ON sessions(user_id);
CREATE INDEX sessions_expires_at_idx ON sessions(expires_at);

CREATE TABLE magic_links (
  token_hash TEXT PRIMARY KEY,
  nonce TEXT NOT NULL,
  email TEXT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  consumed_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX magic_links_email_idx ON magic_links(email);
CREATE INDEX magic_links_expires_at_idx ON magic_links(expires_at);
CREATE INDEX magic_links_unconsumed_idx ON magic_links(token_hash) WHERE consumed_at IS NULL;

CREATE TABLE triggers (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('webhook','chain_event','schedule')),
  action_code TEXT NOT NULL,
  action_cid TEXT NOT NULL,
  default_params JSONB NOT NULL DEFAULT '{}'::jsonb,
  usage_api_key_ciphertext BYTEA NOT NULL,
  usage_api_key_nonce BYTEA NOT NULL,
  chipotle_account_address TEXT,
  max_runs_per_minute INTEGER,
  max_queued_runs INTEGER,
  config JSONB NOT NULL DEFAULT '{}'::jsonb,
  enabled BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX triggers_user_id_idx ON triggers(user_id);
CREATE INDEX triggers_kind_enabled_idx ON triggers(kind, enabled);

CREATE TABLE trigger_runs (
  id UUID PRIMARY KEY,
  trigger_id UUID NOT NULL REFERENCES triggers(id) ON DELETE CASCADE,
  started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  finished_at TIMESTAMPTZ,
  status TEXT NOT NULL CHECK (status IN ('queued','running','success','failed','retrying')),
  input JSONB,
  response JSONB,
  error TEXT,
  attempt INTEGER NOT NULL DEFAULT 1,
  claimed_at TIMESTAMPTZ,
  next_attempt_at TIMESTAMPTZ
);
CREATE INDEX trigger_runs_trigger_id_started_at_idx ON trigger_runs(trigger_id, started_at DESC);
CREATE INDEX trigger_runs_status_idx ON trigger_runs(status);
CREATE INDEX trigger_runs_next_attempt_idx ON trigger_runs(status, next_attempt_at);

CREATE TABLE chain_watermarks (
  trigger_id UUID PRIMARY KEY REFERENCES triggers(id) ON DELETE CASCADE,
  last_block BIGINT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
