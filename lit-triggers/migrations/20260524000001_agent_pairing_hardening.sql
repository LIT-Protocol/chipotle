-- Hardening for the agent-authorization flow (fixes account-takeover via
-- magic-link next-path poisoning + agent-token rebind).
--
-- Before this change the browser supplied the binding target (a raw token hash)
-- directly to POST /agent/authorize, and the upsert moved user_id / cleared
-- revoked_at on conflict. Combined with next-path poisoning that let an attacker
-- bind a token they control to a victim's account with a single click.
--
-- The binding target now lives in server-side state (`agent_pairings`), created
-- by the agent at pairing-start and referenced only by an opaque, single-use,
-- short-lived server-issued code. The browser can no longer choose the hash.

CREATE TABLE IF NOT EXISTS agent_pairings (
  -- sha256 of the opaque server-issued pairing code (the code itself is a secret
  -- carried in the authorize URL and never stored in the clear).
  code_hash TEXT PRIMARY KEY,
  -- The agent-token hash that will be bound once the user approves and the agent
  -- proves possession of the preimage. Recorded at pairing-start, never taken
  -- from the browser at approval time.
  token_hash TEXT NOT NULL,
  -- Short, human-readable code shown to the user so they can confirm this
  -- pairing matches the one their own agent displayed (anti-consent-phishing).
  user_code TEXT NOT NULL,
  label TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL,
  approved_at TIMESTAMPTZ,
  approved_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
  consumed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_agent_pairings_expires_at ON agent_pairings(expires_at);

-- Agent access tokens now expire so a leaked/forgotten token cannot grant
-- indefinite access.
ALTER TABLE agent_access_tokens
  ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;

-- Every token that already exists was minted under the old, vulnerable flow
-- where the browser chose the binding target and existing bindings could be
-- silently rebound. Any of them could be attacker-planted, so invalidate them
-- all and force re-pairing through the hardened flow. Agents re-authorize
-- automatically on their next 401 (see SKILL.md).
--
-- DELETE (not `UPDATE ... SET revoked_at = now()`): the hardened `bind_token`
-- upsert only refreshes a conflicting row `WHERE user_id = EXCLUDED.user_id AND
-- revoked_at IS NULL` and never clears `revoked_at`, so a row left in the
-- revoked state here could never be rebound — the automatic re-pair on the next
-- 401 would hit the surviving revoked row, update zero rows, and 403 forever.
-- Removing the rows outright lets an owner-approved re-pair INSERT a fresh
-- binding for the same token, while a genuinely revoked (still-present) token
-- stays unresurrectable — the leaked-token-rebind protection this migration and
-- #79 add is preserved.
DELETE FROM agent_access_tokens;
