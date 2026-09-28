-- Owners can delete a secret. Deletion drops its policies, registry entry and
-- ciphertext in one transaction; the derived action's Chipotle group grant is
-- retired afterwards (durable intent here, applied under the vault lock, retried
-- by the worker and abandoned after a few failures so a lost response cannot
-- pin the row forever).
ALTER TABLE kc_execution_actions
    ADD COLUMN removing BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN remove_attempts INTEGER NOT NULL DEFAULT 0 CHECK (remove_attempts >= 0);
