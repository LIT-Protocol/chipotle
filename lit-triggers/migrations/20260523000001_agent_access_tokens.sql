CREATE TABLE IF NOT EXISTS agent_access_tokens (
  token_hash TEXT PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  label TEXT NOT NULL DEFAULT 'local-agent',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_used_at TIMESTAMPTZ,
  revoked_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_agent_access_tokens_user_id
  ON agent_access_tokens(user_id);

ALTER TABLE magic_links
  ADD COLUMN IF NOT EXISTS redirect_path TEXT;
