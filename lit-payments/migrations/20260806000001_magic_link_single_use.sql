-- CPL-379 L8: make magic-link tokens single-use.
--
-- Magic-link tokens are stateless HMAC authenticators valid for the full
-- 15-minute TTL, so the same link could be replayed to mint several sessions
-- within that window. Record each token's signature the first time it is
-- verified; a second verify with the same signature is rejected.
--
-- We store the HMAC signature portion of the token (already an authenticator,
-- not additional secret material) rather than the raw payload. `expires_at`
-- mirrors the token's own expiry so a periodic purge can reclaim rows once the
-- token would be rejected as expired anyway.

CREATE TABLE used_magic_links (
    token_sig   TEXT        PRIMARY KEY,
    expires_at  TIMESTAMPTZ NOT NULL,
    used_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX used_magic_links_expires_at_idx ON used_magic_links (expires_at);
