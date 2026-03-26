# TODOS

## P2: Startup Stripe key validation

**What:** Add `stripe::validate_key()` that calls `GET /v1/balance` at startup. Auth errors (401) are fatal (exit). Availability errors (5xx, timeout) are graceful (billing disabled, retry on first billing request).

**Why:** Currently `stripe::init()` only checks if env vars are present and non-empty. If the keys are invalid (revoked, wrong environment), billing silently fails on the first user request instead of at startup.

**Pros:** Immediate feedback on bad keys. Prevents silent billing failures. Catches test keys accidentally used in production (Stripe returns different key prefixes for test vs live).

**Cons:** Adds a network call at startup (~200ms). Requires the Stripe client refactor (preserve HTTP status, add timeouts) which is shipping in the current PR.

**Context:** Identified during CEO plan review (2026-03-26). Codex flagged that the original Stripe client throws away HTTP status codes (stripe.rs:53,80) and uses a no-timeout reqwest::Client, making auth/availability distinction unreliable. The client refactor (shipping in this PR) fixes that prerequisite.

**Effort:** S (human: ~2h / CC: ~5 min)

**Priority:** P2

**Depends on:** Stripe client refactor (HTTP status preservation + request timeouts) — shipping in PR GTC6244/stripe-secrets-ci-docker.
