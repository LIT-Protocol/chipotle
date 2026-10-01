-- Authenticator codes protect hosted login, independently of Lit owner receipts.
ALTER TABLE kc_challenges ADD COLUMN purpose TEXT NOT NULL DEFAULT 'login';
ALTER TABLE kc_challenges ADD COLUMN session_hash TEXT;

CREATE TABLE kc_two_factor (
    vault_id TEXT PRIMARY KEY REFERENCES kc_vaults(id),
    encrypted_secret TEXT NOT NULL,
    last_step BIGINT NOT NULL,
    enabled_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE kc_two_factor_setup (
    vault_id TEXT PRIMARY KEY REFERENCES kc_vaults(id),
    session_hash TEXT NOT NULL,
    encrypted_secret TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE kc_two_factor_recovery (
    vault_id TEXT NOT NULL REFERENCES kc_vaults(id),
    code_hash TEXT NOT NULL,
    PRIMARY KEY(vault_id, code_hash)
);
CREATE TABLE kc_two_factor_logins (
    token_hash TEXT PRIMARY KEY,
    vault_id TEXT NOT NULL REFERENCES kc_vaults(id),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX kc_two_factor_logins_vault ON kc_two_factor_logins(vault_id);
CREATE INDEX kc_two_factor_logins_expiry ON kc_two_factor_logins(expires_at);
