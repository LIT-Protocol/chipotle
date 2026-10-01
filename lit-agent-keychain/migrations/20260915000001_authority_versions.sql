-- Authority action releases are versioned. A vault keeps working across releases:
-- kc_vaults.authority_cid is the newest release the owner has signed in with, and
-- kc_vault_authorities records every release CID granted to the vault's fixed
-- Chipotle group, so old secrets (which pin the release they were created under)
-- stay manageable after an upgrade.
CREATE TABLE kc_vault_authorities (
    vault_id TEXT NOT NULL REFERENCES kc_vaults(id) ON DELETE CASCADE,
    authority_cid TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (vault_id, authority_cid)
);
INSERT INTO kc_vault_authorities(vault_id, authority_cid) SELECT id, authority_cid FROM kc_vaults;
