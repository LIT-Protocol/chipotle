-- Lit Chat admin plane (plans/tee-chat-app.md section 4.4).
-- Same database, but the admin service connects as a separate role whose
-- grants cover only these tables (see db/roles.sql). A fully compromised
-- admin plane can spend money and break inference; it cannot read chats.

CREATE TABLE provider_keys (
  id UUID PRIMARY KEY,
  provider TEXT NOT NULL CHECK (provider IN ('openrouter')),
  kind TEXT NOT NULL CHECK (kind IN ('provisioning', 'runtime')),
  -- AES-256-GCM under the service KEK get_key("chat/v1/provider-keys", "chat-svc").
  -- AAD binds (key_id, provider, kind). Write-only: there is no reveal path.
  enc_key BYTEA NOT NULL,
  -- The ONLY representation any API response, log line, audit row, or UI
  -- element ever renders (first 8 / last 4).
  masked_hint TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('active', 'standby', 'retiring', 'disabled')),
  spend_limit_usd DOUBLE PRECISION,
  -- OpenRouter's key hash, used to drive the provisioning API (disable/delete).
  upstream_hash TEXT,
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  rotated_at TIMESTAMPTZ
);
CREATE INDEX provider_keys_lookup_idx ON provider_keys(provider, kind, status);

-- Admin roster. A bare DB row is never the root of trust: each row carries an
-- enclave MAC over (user_ref_hash, role, granted_by, granted_at_unix) under
-- get_key("chat/v1/admin-roster-mac", "chat-svc"). Rows without a valid MAC
-- are ignored, so an operator with DB write access cannot grant admin.
CREATE TABLE admins (
  user_ref_hash TEXT PRIMARY KEY,
  role TEXT NOT NULL CHECK (role IN ('admin')),
  granted_by TEXT NOT NULL,
  granted_at_unix BIGINT NOT NULL,
  mac BYTEA NOT NULL
);

-- WebAuthn passkeys for the mandatory admin second factor. Credential blobs
-- are ciphertext under a service KEK; the DB operator cannot read or swap
-- them (AAD binds cred_id + user_ref_hash).
CREATE TABLE admin_credentials (
  cred_id TEXT PRIMARY KEY,
  user_ref_hash TEXT NOT NULL,
  enc_passkey BYTEA NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX admin_credentials_user_idx ON admin_credentials(user_ref_hash);

-- Append-only, TEE-MAC'd, hash-chained audit log. The chain makes edits
-- evident, not deletions (a DB operator can truncate the tail; disclosed).
CREATE TABLE admin_audit_log (
  id BIGSERIAL PRIMARY KEY,
  actor_ref_hash TEXT NOT NULL,
  action TEXT NOT NULL,
  subject TEXT NOT NULL,
  -- Masked hints only; never key material.
  detail TEXT NOT NULL,
  prev_hash BYTEA NOT NULL,
  row_hash BYTEA NOT NULL,
  mac BYTEA NOT NULL,
  created_at_unix BIGINT NOT NULL
);

-- Small runtime-tunable settings (spend caps, breaker state). Version column
-- backs a short-TTL cache in lit-chat so changes take effect within seconds.
CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  version BIGINT NOT NULL DEFAULT 1,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
