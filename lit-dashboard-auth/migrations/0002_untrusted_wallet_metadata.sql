-- Wallet addresses are unverified client metadata, not global account identity.
-- Rebuild atomically while preserving users and the sessions that reference them.
PRAGMA defer_foreign_keys = ON;
CREATE TABLE auth_users_new (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  salt TEXT NOT NULL,
  verifier TEXT,
  version INTEGER NOT NULL DEFAULT 1,
  state TEXT NOT NULL DEFAULT 'verified' CHECK(state IN ('verified','reserved','creating','active')),
  account TEXT,
  envelope TEXT,
  operation TEXT,
  created_at INTEGER NOT NULL,
  CHECK ((state = 'active') = (envelope IS NOT NULL)),
  CHECK (state != 'active' OR account IS NOT NULL)
);
INSERT INTO auth_users_new (id,email,salt,verifier,version,state,account,envelope,operation,created_at)
SELECT id,email,salt,verifier,version,state,account,envelope,operation,created_at FROM auth_users;
DROP TABLE auth_users;
ALTER TABLE auth_users_new RENAME TO auth_users;
PRAGMA defer_foreign_keys = OFF;
