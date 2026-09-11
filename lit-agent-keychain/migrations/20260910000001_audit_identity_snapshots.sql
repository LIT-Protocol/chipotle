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
