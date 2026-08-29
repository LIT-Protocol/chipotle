-- Users / sessions / magic links / agent access tokens: same shape as lit-triggers.
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
  redirect_path TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX magic_links_email_idx ON magic_links(email);
CREATE INDEX magic_links_expires_at_idx ON magic_links(expires_at);
CREATE INDEX magic_links_unconsumed_idx ON magic_links(token_hash) WHERE consumed_at IS NULL;

CREATE TABLE agent_access_tokens (
  token_hash TEXT PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  label TEXT NOT NULL DEFAULT 'local-agent',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_used_at TIMESTAMPTZ,
  revoked_at TIMESTAMPTZ
);
CREATE INDEX agent_access_tokens_user_id_idx ON agent_access_tokens(user_id);

-- One tenant per user: a vault PKP + a Chipotle group on the app's account.
-- The service usage key lets the app run the encrypt action for this tenant.
CREATE TABLE tenants (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
  pkp_id TEXT NOT NULL,
  group_id BIGINT NOT NULL,
  service_key_ciphertext BYTEA NOT NULL,
  service_key_nonce BYTEA NOT NULL,
  reader_cid TEXT NOT NULL,
  encrypt_cid TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Actions registered on the app's Chipotle account (add_action is per account,
-- so it only needs to happen once per CID regardless of tenant).
CREATE TABLE account_actions (
  cid TEXT PRIMARY KEY,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Customer-owned actions permitted in a tenant's group (in-TEE-only tier).
CREATE TABLE tenant_actions (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  cid TEXT NOT NULL,
  name TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, cid)
);

CREATE TABLE secrets (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'generic',
  environment TEXT NOT NULL DEFAULT 'production',
  release TEXT NOT NULL CHECK (release IN ('plaintext', 'in_tee_only')),
  policy JSONB NOT NULL DEFAULT '{}'::jsonb,
  current_version INTEGER NOT NULL DEFAULT 1,
  disabled BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, name)
);
CREATE INDEX secrets_tenant_id_idx ON secrets(tenant_id);

-- Ciphertexts are sealed to the tenant's vault PKP inside Chipotle's TEE; a
-- copy of this table leaks nothing usable.
CREATE TABLE secret_versions (
  id UUID PRIMARY KEY,
  secret_id UUID NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
  version INTEGER NOT NULL,
  ciphertext TEXT NOT NULL,
  ciphertext_hash TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (secret_id, version)
);

-- Agents = scoped Chipotle usage API keys minted by the app. The plaintext key
-- is returned once; we keep a hash for auth and an encrypted copy for revocation
-- (remove_usage_api_key needs the plaintext).
CREATE TABLE agents (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  usage_key_hash TEXT NOT NULL UNIQUE,
  usage_key_ciphertext BYTEA NOT NULL,
  usage_key_nonce BYTEA NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen_at TIMESTAMPTZ,
  revoked_at TIMESTAMPTZ
);
CREATE INDEX agents_tenant_id_idx ON agents(tenant_id);

CREATE TABLE access_log (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  secret_id UUID REFERENCES secrets(id) ON DELETE SET NULL,
  agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
  event TEXT NOT NULL,
  decision TEXT NOT NULL CHECK (decision IN ('allow', 'deny')),
  reason TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX access_log_tenant_created_idx ON access_log(tenant_id, created_at DESC);
CREATE INDEX access_log_secret_event_created_idx ON access_log(secret_id, event, created_at DESC);
