-- Single-use enforcement for magic-link tokens (CPL-379 L8).
--
-- Magic-link tokens are stateless HMAC-signed blobs (see auth/token.rs), so on
-- their own they can be replayed any number of times within their 15-minute
-- TTL. This table records the hash of every token the moment it is first
-- redeemed; the verify route inserts ON CONFLICT DO NOTHING and treats a
-- conflict (row already present) as "already used", rejecting the replay.
--
-- Only the SHA-256 hash of the token is stored, never the token itself, so a
-- read of this table cannot mint a session. Rows are needed only until the
-- token would expire on its own (after that the TTL check in token::verify
-- rejects it anyway); expires_at drives periodic cleanup.

CREATE TABLE used_magic_links (
    token_hash TEXT        PRIMARY KEY,
    email      TEXT        NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Supports the periodic cleanup of consumed tokens past their original expiry.
CREATE INDEX used_magic_links_expires_at_idx ON used_magic_links (expires_at);
