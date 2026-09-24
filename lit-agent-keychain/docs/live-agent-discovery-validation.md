# Live discovery validation

Validated locally on September 24, 2026. No production API, npm publication, merge or push was performed.

## Design and boundaries

- `LiveKeychain(privateKey, { serviceUrl? })` discovers on every list/get/use. CLI, MCP and run default to this path when no legacy config is supplied. Static `Keychain` remains compatible.
- Dedicated audience/domain-bound Ed25519 proofs sign canonical SHA-256 challenge digests. PostgreSQL atomically consumes 60-second challenges; global/peer budgets and existing JSON limits bound ingress. Discovery rejects inventories above 1,000 candidates rather than silently truncating.
- Only current enabled/unexpired, correctly versioned owner-receipted grants are disclosed. SDK verifies manifests, envelope names, CIDs and policies against its independently configured Lit origin. Ambiguous names require qualified vault/secret IDs.
- Discovery supplies only the approved vault's existing execution-only billing credential. It never supplies account-management/master credentials, private identity keys or plaintext. This reusable billing key is not secret-access authority: retained keys may consume sponsored execution until rotation. Owner authorization remains enforced by unchanged Lit actions. See SECURITY.md for this and existing operator rollback trust.
- Updates take effect on the next request, not already returned plaintext or already-authorized in-flight work. No archived action/catalog changes.

## Actual local results

Logs are in `/home/chris/.hermes/cache/scratch/` (not committed).

| Check | Result | Log |
| --- | --- | --- |
| `npm run build` | Passed; existing Vite large-chunk warning | `live-final-build.log` |
| `npm test` | 117 passed, 4 skipped, 0 failed (121 tests) | `live-final-test.log` |
| `npm run test:owner-onboarding` | 6 passed | `live-final-ui.log` |
| isolated passkey Playwright config | 10 passed | `live-final-passkey.log` |
| `cargo +1.91 fmt --check` | Passed | `live-final-fmt.log` |
| `cargo +1.91 clippy --all-targets -- -D warnings` | Passed | `live-final-clippy.log` |
| `cargo +1.91 test` | All targets passed; live-production smoke remains ignored | `live-final-rust.log` |
| `node scripts/test-local.mjs` with local test PostgreSQL | 4 API suites passed (including live discovery), Rust passed, 3 browser tests passed | `live-final-integration.log` |
| `npm run test:runtime` | Actual Deno runtime encrypted release, response signature and HPKE verified | `live-runtime.log` |
| `node scripts/publish-sdk.mjs --check` | Passed: local 2.1.0, published 2.0.7; merge will publish 2.1.0 | `live-final-publish.log` |
| `git diff --check` | Passed | terminal output |

The real local fixture uses PostgreSQL, the Rust service and repository mock Lit/Stripe. It exercises owner-signed writes and full action authorization rather than bypassing security checks. The separate runtime check covers actual Deno execution. Discovery API tests are added to the existing local-fixture script used by CI. No live production/enclave end-to-end test was run.

A local PostgreSQL 15.19 package was extracted into the scratch directory because server binaries were initially absent; the dedicated database runs on loopback port 55439. No production fallback was used.

## Remaining review

Parent independent review and remote CI remain required before merge. In particular review the explicit reusable execution-billing credential exposure/rotation boundary, ingress budgets, and the 1,000-candidate limit. The new SDK uses additional fresh discovery/bundle requests (no inventory cache); this trades request volume for next-operation visibility and verification.
