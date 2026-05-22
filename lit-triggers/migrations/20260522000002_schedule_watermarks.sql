CREATE TABLE schedule_watermarks (
  trigger_id UUID PRIMARY KEY REFERENCES triggers(id) ON DELETE CASCADE,
  last_enqueued_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX schedule_watermarks_updated_at_idx ON schedule_watermarks(updated_at);
