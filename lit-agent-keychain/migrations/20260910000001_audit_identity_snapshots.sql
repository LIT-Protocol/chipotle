-- Access-log rows must stay attributable to the secret they were about even
-- after that secret is deleted (KC-04). Snapshot the names at write time and
-- stop nulling secret_id on delete: a recreated secret with the same name gets
-- a fresh UUID, so historical rows remain distinguishable from the new one.
ALTER TABLE access_log DROP CONSTRAINT IF EXISTS access_log_secret_id_fkey;
ALTER TABLE access_log
  ADD COLUMN secret_name TEXT,
  ADD COLUMN agent_name TEXT;

UPDATE access_log l SET secret_name = s.name FROM secrets s WHERE s.id = l.secret_id;
UPDATE access_log l SET agent_name = a.name FROM agents a WHERE a.id = l.agent_id;

-- Active agent names are unique per tenant (KC-03). Existing duplicates keep
-- their keys but get a "-<id suffix>" so the index can be created; the app
-- checks first and maps a unique violation to 409 agent_name_exists.
UPDATE agents a
SET name = a.name || '-' || right(a.id::text, 4)
WHERE a.revoked_at IS NULL
  AND EXISTS (
    SELECT 1 FROM agents b
    WHERE b.tenant_id = a.tenant_id AND b.name = a.name
      AND b.revoked_at IS NULL AND b.created_at < a.created_at
  );
CREATE UNIQUE INDEX agents_tenant_active_name_idx
  ON agents (tenant_id, name) WHERE revoked_at IS NULL;
