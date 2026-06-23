-- Gas funder: keep the lit-api-server API payer pool topped up.
--
-- lit-api-server signs `new_account` (and other write) transactions from a
-- pool of payer wallets whose keys live inside the dstack TEE. The pool's
-- signer selection is NOT balance-aware: a drained payer is still handed
-- requests, and the on-chain `newAccount` then reverts with
-- `insufficient funds for gas`. There is no in-TEE continuous top-up — the
-- admin payer only rebalances on pool *resize*.
--
-- lit-payments runs out of the TEE hot path (Railway, single instance,
-- already polling), so it carries a small hot wallet that tops up any payer
-- below a low-water mark, up to a high-water target. These two tables are the
-- audit ledger (which also backs the rolling 24h spend cap) and the alert
-- de-dupe state. See `src/gas_funder/`.

-- One row per attempted funding transaction. `status='pending'` is written
-- BEFORE broadcast so a crash mid-send is conservatively counted against the
-- daily cap (errs toward NOT over-funding) and surfaces as a stale-pending
-- warning rather than silently double-spending on restart.
CREATE TABLE gas_funding_events (
    id                  BIGSERIAL   PRIMARY KEY,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    chain_id            BIGINT      NOT NULL,
    -- 0x-lowercased recipient (an API payer pool wallet or the admin payer).
    recipient           TEXT        NOT NULL,
    -- Amount sent, in wei. NUMERIC holds the full 256-bit range; we bind/read
    -- it as a base-10 string (U256) to avoid lossy float conversions.
    amount_wei          NUMERIC     NOT NULL CHECK (amount_wei > 0),
    -- Recipient balance observed just before the send, for the audit trail.
    balance_before_wei  NUMERIC,
    tx_hash             TEXT,
    -- 'pending' (recorded, not yet confirmed) | 'sent' (receipt, status 1)
    -- | 'failed' (send error / revert / receipt error).
    status              TEXT        NOT NULL CHECK (status IN ('pending', 'sent', 'failed')),
    error               TEXT
);

-- Rolling-window sum for the daily cap query (created_at > now() - 24h).
CREATE INDEX gas_funding_events_created_at_idx ON gas_funding_events (created_at DESC);

-- Small partial index to find interrupted sends (status='pending' that never
-- transitioned) for the stale-pending warning.
CREATE INDEX gas_funding_events_pending_idx
    ON gas_funding_events (created_at)
    WHERE status = 'pending';

-- One row per alert kind/key, holding the last time we emailed it. The
-- funder upserts here under a cooldown so a persistently-low wallet emails
-- once per cooldown window instead of on every poll tick.
CREATE TABLE gas_funder_alerts (
    alert_key     TEXT        PRIMARY KEY,
    last_sent_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
