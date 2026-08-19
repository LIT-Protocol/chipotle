-- Lit Chat data plane (plans/tee-chat-app.md section 4.3).
-- Content columns are ciphertext (BYTEA); plaintext is limited to
-- routing/ordering metadata. The DB never sees an email, a raw user_ref,
-- or a message body.

CREATE TABLE chat_users (
  user_ref_hash TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('anon', 'account')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE conversations (
  id UUID PRIMARY KEY,
  user_ref_hash TEXT NOT NULL REFERENCES chat_users(user_ref_hash) ON DELETE CASCADE,
  -- Random per-conversation DEK, AES-256-GCM-wrapped by the user's
  -- enclave-derived KEK. AAD binds (user_ref_hash, conversation_id).
  wrapped_dek BYTEA NOT NULL,
  -- Title ciphertext under the DEK. AAD binds conversation_id.
  enc_title BYTEA,
  model_id TEXT NOT NULL,
  -- Optimistic concurrency for multi-tab/multi-device (expected_version -> 409).
  version BIGINT NOT NULL DEFAULT 1,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX conversations_user_idx ON conversations(user_ref_hash, updated_at DESC);

CREATE TABLE messages (
  id UUID PRIMARY KEY,
  conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  seq BIGINT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
  -- AES-256-GCM under the conversation DEK.
  -- AAD binds (conversation_id, message_id, seq, role): a moved, reordered,
  -- or role-flipped ciphertext fails decryption.
  ciphertext BYTEA NOT NULL,
  -- Token-usage metadata is content-adjacent, so it is encrypted too.
  enc_usage_meta BYTEA,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (conversation_id, seq)
);

-- Sessions are TEE-signed, never DB-rooted (section 5.1). The DB holds at
-- most a revocation list; an operator with DB write access cannot mint a
-- valid session by inserting rows here.
CREATE TABLE sessions_revoked (
  token_hash TEXT PRIMARY KEY,
  revoked_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Replay guard only. Identity comes from the enclave-signed token; this row
-- holds nothing identifying, so a rewritten row cannot repoint a login.
CREATE TABLE magic_links (
  token_hash TEXT PRIMARY KEY,
  expires_at TIMESTAMPTZ NOT NULL,
  used_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX magic_links_expires_idx ON magic_links(expires_at);

-- Per-user micro-USD spend accumulator + anonymous daily token budget
-- (section 8.2). Content-adjacent, so ciphertext under the user KEK.
CREATE TABLE user_meters (
  user_ref_hash TEXT PRIMARY KEY REFERENCES chat_users(user_ref_hash) ON DELETE CASCADE,
  enc_meter BYTEA NOT NULL,
  version BIGINT NOT NULL DEFAULT 1,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Aggregate, non-attributable ops counters (per-day totals only) backing the
-- global spend circuit breaker. Deliberately plaintext: nothing here maps to
-- a user or a conversation.
CREATE TABLE spend_days (
  day TEXT PRIMARY KEY,
  micro_usd BIGINT NOT NULL DEFAULT 0,
  requests BIGINT NOT NULL DEFAULT 0
);

-- Curated OpenRouter subset with per-model privacy labeling (section 4.2
-- catalog policy). zdr = routed only to zero-data-retention providers.
CREATE TABLE model_catalog (
  model_id TEXT PRIMARY KEY,
  display_name TEXT NOT NULL,
  zdr BOOLEAN NOT NULL DEFAULT true,
  enabled BOOLEAN NOT NULL DEFAULT true,
  prompt_usd_per_mtok DOUBLE PRECISION,
  completion_usd_per_mtok DOUBLE PRECISION,
  notes TEXT,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO model_catalog (model_id, display_name, zdr, enabled, prompt_usd_per_mtok, completion_usd_per_mtok, notes) VALUES
  ('openai/gpt-4o-mini',                 'GPT-4o mini',          true, true, 0.15,  0.60,  'Default model'),
  ('anthropic/claude-sonnet-4.5',        'Claude Sonnet 4.5',    true, true, 3.00, 15.00,  NULL),
  ('anthropic/claude-haiku-4.5',         'Claude Haiku 4.5',     true, true, 1.00,  5.00,  NULL),
  ('meta-llama/llama-3.3-70b-instruct',  'Llama 3.3 70B',        true, true, 0.30,  0.40,  NULL),
  ('mistralai/mistral-small-3.2-24b-instruct', 'Mistral Small 3.2', true, true, 0.10, 0.30, NULL);
