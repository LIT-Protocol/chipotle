-- LITKEY on-chain payment records. Successful credits are idempotent by
-- chain + tx hash + log index so WSS and reconciliation poller races cannot
-- double-credit the same Payment event.
CREATE TABLE litkey_payments (
    id BIGSERIAL PRIMARY KEY,
    chain_id BIGINT NOT NULL,
    gateway_address TEXT NOT NULL,
    tx_hash TEXT NOT NULL,
    log_index BIGINT NOT NULL CHECK (log_index >= 0),
    block_number BIGINT NOT NULL CHECK (block_number >= 0),
    wallet_address TEXT NOT NULL,
    payer_address TEXT NOT NULL,
    litkey_amount_wei NUMERIC(78, 0) NOT NULL CHECK (litkey_amount_wei > 0),
    usd_wei_per_litkey NUMERIC(78, 0) NOT NULL CHECK (usd_wei_per_litkey > 0),
    discount_basis_points BIGINT NOT NULL CHECK (discount_basis_points BETWEEN 0 AND 9000),
    cents_credited BIGINT NOT NULL CHECK (cents_credited > 0),
    stripe_customer_id TEXT NOT NULL,
    stripe_balance_transaction_id TEXT NOT NULL,
    credited_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (chain_id, tx_hash, log_index)
);

CREATE INDEX litkey_payments_wallet_credited_at_idx
    ON litkey_payments (wallet_address, credited_at DESC);

CREATE INDEX litkey_payments_credited_at_idx
    ON litkey_payments (credited_at DESC);

-- Reconciliation checkpoint. Advanced only by the HTTPS poll path after a
-- whole confirmed range has been processed successfully.
CREATE TABLE chain_checkpoint (
    chain_id BIGINT PRIMARY KEY,
    last_processed_block BIGINT NOT NULL CHECK (last_processed_block >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
