-- Billing controls storage/sponsorship only; it never creates owner permissions.
CREATE TABLE kc_subscriptions (
    vault_id TEXT PRIMARY KEY REFERENCES kc_vaults(id),
    customer_id TEXT UNIQUE,
    subscription_id TEXT,
    status TEXT NOT NULL DEFAULT 'none',
    paid_until TIMESTAMPTZ,
    cancel_at_period_end BOOLEAN NOT NULL DEFAULT false,
    checkout_id TEXT,
    checkout_generation BIGINT NOT NULL DEFAULT 0,
    custom_secret_limit BIGINT CHECK (custom_secret_limit BETWEEN 1 AND 100000),
    custom_until TIMESTAMPTZ,
    synced_at TIMESTAMPTZ,
    reconcile_after TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE kc_stripe_events (
    id TEXT PRIMARY KEY,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE kc_execution_accounts (
    vault_id TEXT PRIMARY KEY REFERENCES kc_vaults(id),
    group_id BIGINT NOT NULL CHECK (group_id > 0),
    secret_group_id BIGINT NOT NULL CHECK (secret_group_id > 0),
    encrypted_key TEXT,
    revoking_key TEXT,
    scope_hash TEXT
);
CREATE TABLE kc_execution_actions (
    vault_id TEXT NOT NULL REFERENCES kc_vaults(id),
    secret_id TEXT NOT NULL,
    action_cid TEXT NOT NULL,
    applied BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (vault_id, secret_id),
    UNIQUE (vault_id, action_cid)
);
