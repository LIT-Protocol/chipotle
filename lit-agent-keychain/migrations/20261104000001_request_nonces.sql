-- Claims survive failed executions, restarts and secret deletion/restoration.
CREATE TABLE kc_used_requests (
    vault_id TEXT NOT NULL REFERENCES kc_vaults(id) ON DELETE CASCADE,
    agent_public_key TEXT NOT NULL,
    operation TEXT NOT NULL,
    nonce TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (vault_id, agent_public_key, operation, nonce)
);
CREATE INDEX kc_used_requests_expiry ON kc_used_requests(expires_at);
