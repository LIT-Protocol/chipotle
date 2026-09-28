-- Independent from owner sessions; a proof authorizes one discovery only.
CREATE TABLE kc_agent_challenges (
    nonce TEXT PRIMARY KEY CHECK (length(nonce) = 64),
    challenge JSONB NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX kc_agent_challenges_expiry ON kc_agent_challenges(expires_at);
CREATE INDEX kc_policy_agent_grants ON kc_policies USING gin ((signed->'document'->'grants') jsonb_path_ops);
