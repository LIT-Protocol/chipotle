-- Intentional prelaunch replacement. Legacy operator grants and managed vaults
-- are not migrated: their authority model cannot satisfy v2 guarantees.
DROP TABLE access_log, agents, secret_versions, secrets, tenant_actions,
    account_actions, tenants, agent_access_tokens, magic_links, sessions, users;

CREATE TABLE kc_vaults (
    id TEXT PRIMARY KEY, authority JSONB NOT NULL, authority_cid TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE kc_challenges (
    challenge TEXT PRIMARY KEY, vault_id TEXT NOT NULL, expires_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE kc_sessions (
    token_hash TEXT PRIMARY KEY, vault_id TEXT NOT NULL REFERENCES kc_vaults(id),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX kc_sessions_expiry ON kc_sessions(expires_at);
CREATE TABLE kc_secrets (
    id TEXT PRIMARY KEY, vault_id TEXT NOT NULL REFERENCES kc_vaults(id),
    name TEXT NOT NULL, manifest JSONB NOT NULL, action_cid TEXT NOT NULL,
    current_version BIGINT NOT NULL CHECK (current_version > 0),
    archived BOOLEAN NOT NULL DEFAULT false, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(vault_id, name)
);
CREATE TABLE kc_envelopes (
    secret_id TEXT NOT NULL REFERENCES kc_secrets(id) ON DELETE CASCADE,
    version BIGINT NOT NULL, envelope_hash TEXT NOT NULL, signed JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY(secret_id, version)
);
CREATE TABLE kc_policies (
    hash TEXT PRIMARY KEY, vault_id TEXT NOT NULL REFERENCES kc_vaults(id),
    scope TEXT NOT NULL, epoch BIGINT NOT NULL CHECK (epoch > 0), signed JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE kc_registry (
    scope TEXT PRIMARY KEY, vault_id TEXT NOT NULL REFERENCES kc_vaults(id),
    policy_hash TEXT NOT NULL REFERENCES kc_policies(hash), epoch BIGINT NOT NULL CHECK (epoch > 0)
);
CREATE TABLE kc_audit (
    id BIGSERIAL PRIMARY KEY, vault_id TEXT NOT NULL REFERENCES kc_vaults(id),
    event TEXT NOT NULL, object_hash TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX kc_audit_vault_cursor ON kc_audit(vault_id, id DESC);
CREATE TABLE kc_budgets (
    bucket TEXT NOT NULL, period_start BIGINT NOT NULL, used BIGINT NOT NULL CHECK (used > 0),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT now() + interval '2 days',
    PRIMARY KEY(bucket, period_start)
);
CREATE INDEX kc_passkey_discovery ON kc_vaults ((authority->'owner'->>'credentialId'))
    WHERE authority->'owner'->>'kind'='passkey';
