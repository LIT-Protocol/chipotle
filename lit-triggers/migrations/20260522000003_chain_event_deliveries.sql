CREATE TABLE chain_event_deliveries (
  id BIGSERIAL PRIMARY KEY,
  trigger_id UUID NOT NULL REFERENCES triggers(id) ON DELETE CASCADE,
  chain_id BIGINT NOT NULL,
  tx_hash TEXT NOT NULL,
  log_index BIGINT NOT NULL,
  delivery_key TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (trigger_id, chain_id, tx_hash, log_index)
);
CREATE INDEX chain_event_deliveries_trigger_id_created_at_idx
  ON chain_event_deliveries(trigger_id, created_at DESC);
