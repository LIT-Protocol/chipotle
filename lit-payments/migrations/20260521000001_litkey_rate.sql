-- Current USD-denominated LITKEY rate, stored as integer cents per LITKEY.
-- Single-row table: id is always 1.
CREATE TABLE litkey_rate (
    id BIGINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    cents_per_litkey BIGINT NOT NULL CHECK (cents_per_litkey BETWEEN 1 AND 1000000),
    source TEXT NOT NULL CHECK (source IN ('coingecko', 'manual')),
    fetched_at TIMESTAMPTZ NOT NULL,
    updated_by_operator_id BIGINT REFERENCES operators(id),
    CONSTRAINT litkey_rate_manual_operator_check CHECK (
        (source = 'manual' AND updated_by_operator_id IS NOT NULL)
        OR (source = 'coingecko' AND updated_by_operator_id IS NULL)
    )
);
