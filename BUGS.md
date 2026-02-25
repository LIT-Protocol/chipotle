# Server Bug Tracker

Issues found while implementing k6 integration tests against the spec.
Tests are written to match the OpenAPI spec; deviations are server-side bugs.

---

## BUG-001: `POST /new_account` returns 500 on configured deployment

**Endpoint:** `POST /new_account`
**Spec expectation:** 200 with `{ api_key, wallet_address }`
**Observed:** 500 — `Internal server error.: Contract call reverted with data: 0xd4a84737...`
**Condition:** Reproduces on the Phala deployment when the AccountConfig contract
is not properly funded / configured for the deployer address.
**Impact:** All downstream tests that require an `api_key` cannot run without a
pre-existing key supplied via `LIT_API_KEY` env var.
**Workaround:** Set `LIT_API_KEY=<base64key>` before running tests.
