-- Per-API-key spending rules + rolling usage for Lambda-parity blast-radius
-- controls on frontend-callable usage keys. See plans/chipotle-lambda-parity.md.
--
-- The gateway (lit-api-server) reads `spending_rules` (cached, SWR) to enforce a
-- rolling spend cap, rate/concurrency limits, and an origin allowlist on keys
-- whose on-chain `hasSpendingRules` flag is set, and increments `spending_usage`
-- off the response path via the internal charge endpoint. Keys with no row here
-- are unaffected — the gateway never reaches this table unless the flag is set.
--
-- `api_key_hash` is the keccak256 of the API key as a 0x-prefixed 32-byte hex
-- string (the same on-chain account identity used elsewhere), stored lowercase.

CREATE TABLE spending_rules (
    api_key_hash            TEXT        PRIMARY KEY,
    -- Billing/account wallet this key belongs to. Audit + grouping only.
    account_wallet_address  TEXT,

    -- Rolling spend cap (AWS-Budgets style). Both NULL = no spend cap.
    spend_cap_cents         BIGINT      CHECK (spend_cap_cents IS NULL OR spend_cap_cents > 0),
    spend_window_seconds    BIGINT      CHECK (spend_window_seconds IS NULL OR spend_window_seconds > 0),

    -- Per-key rate limit (token bucket). Both NULL = no rate limit.
    rate_limit_rps          INTEGER     CHECK (rate_limit_rps IS NULL OR rate_limit_rps > 0),
    rate_limit_burst        INTEGER     CHECK (rate_limit_burst IS NULL OR rate_limit_burst > 0),

    -- Max simultaneous in-flight executions. NULL = no concurrency cap.
    max_concurrency         INTEGER     CHECK (max_concurrency IS NULL OR max_concurrency > 0),

    -- Browser origin allowlist (defense-in-depth). NULL/empty = no restriction.
    allowed_origins         TEXT[],

    -- Lets an operator disable a key's rules without deleting them.
    enabled                 BOOLEAN     NOT NULL DEFAULT TRUE,

    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- A spend cap needs both halves or neither.
    CONSTRAINT spend_cap_complete CHECK (
        (spend_cap_cents IS NULL) = (spend_window_seconds IS NULL)
    ),
    -- A rate limit needs both halves or neither.
    CONSTRAINT rate_limit_complete CHECK (
        (rate_limit_rps IS NULL) = (rate_limit_burst IS NULL)
    )
);

CREATE INDEX spending_rules_wallet_idx ON spending_rules (account_wallet_address);

-- Durable rolling spend counter, one row per key. Independent of spending_rules
-- (no FK) so the gateway's best-effort async charge never fails on a delete
-- race; orphan counters are harmless and cleared when rules are deleted.
CREATE TABLE spending_usage (
    api_key_hash      TEXT        PRIMARY KEY,
    -- Anchor of the current rolling window; reset when the window elapses.
    window_started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    spent_cents       BIGINT      NOT NULL DEFAULT 0 CHECK (spent_cents >= 0),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
