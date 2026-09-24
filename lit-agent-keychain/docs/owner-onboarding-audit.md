# Keychain owner-onboarding audit

## Scope

Owner website (`main.tsx`, `AddSecret.tsx`, `Landing.tsx`, identity/sign-in helpers),
agent-config handoff, related SDK delegation/error guidance, and public onboarding
instructions. No production account, real agent key, provider credential, database,
action template, authorization verifier, deployment, or operator endpoint was changed.

## Findings and fixes

| Finding                                                                                                                                              | Resolution                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ---------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Adding an existing agent key was buried inside a selected secret. Owners arriving with a key had no visible starting point.                          | Primary **+ Add agent** action on Secrets, including an empty vault. A dedicated form guides name/public key → explicit secret selection → approval results/config download. Empty vaults link to Add secret.                                                                                                                                                                                                                                                   |
| The website exposed service-managed execution-key rotation beside subscription controls.                                                             | Removed **Execution and account access**, its rotation button and browser API call. Kept subscription management, secret limits, fair-use/no-overage disclosure, and config privacy guidance. The backend endpoint remains unchanged.                                                                                                                                                                                                                           |
| “Config · all secrets” sounded like a broad authorization action.                                                                                    | Renamed to **Download agent config**. The existing per-agent download still enumerates prior grants, never creates them. The new onboarding handoff lists only successfully approved selections.                                                                                                                                                                                                                                                                |
| Bulk approvals could accidentally broaden access or obscure partial failure.                                                                         | Nothing preselected; **Select all** explicitly selects eligible secrets and **Clear selection** resets the choice without approving. Validate public-key shape/name before work, re-fetch every selected bundle before signatures, block disabled/expired policies. Report partial results, stop on first failure, and re-fetch/skip existing grants on retry. Warn that closing does not revoke saved approvals and an unconfirmed request may have committed. |
| SDK `delegate` normally resets the secret-wide permission expiry to 30 days. Using it unchanged would silently renew other agents during onboarding. | Added opt-in `preserveExpiry` to `delegate`/`setPolicy`, used only by the new flow. Preserve exact finite/null expiry and unrelated grants; retain legacy lifetime caps and signed owner authorization. Existing SDK callers retain their defaults. Tests exercise real SDK policy construction with simulated signer/transport.                                                                                                                                |
| Public guides and SDK 401 errors pointed to removed/unclear UI labels.                                                                               | Updated SKILL.md, llms.txt, README, quickstart, agents reference and 401 guidance. Rejected execution keys now direct owners to config download/support, not a removed rotation screen.                                                                                                                                                                                                                                                                         |

## Owner controls deliberately retained

- Per-secret approval/revocation, disable/enable, rotation, renewal and no-expiry choice.
- Plaintext export confirmation for stored secrets only; connected services remain non-exportable.
- Recovery credentials, encrypted backup/restore and billing management.
- Security/trust disclosures, incomplete-audit warning and collapsed verification details
  (action CID/ciphertext digest). These explain owner risk or support verification;
  they are not operator setup chores. Developer examples remain collapsed, not part
  of the new primary onboarding path.
- No private identity upload, automatic key generation for the owner, automatic
  all-secret access, automatic enable/renew, or implicit plaintext download.

## Verification

Run from `lit-agent-keychain`:

- `npm run build`: passed (action lock checks, SDK build, TypeScript, production Vite build).
- `npm test`: 105 passed, 3 environment-dependent API/billing tests skipped, 0 failed.
- `npm run test:owner-onboarding`: 6 Chromium tests passed (including select-all/clear-selection and eligibility exclusions). Covers the actual App
  sign-in-to-Add-agent wiring with a test-only identity/transport substitute;
  component selection, config content/download, malformed input, cancellation,
  partial rejection/retry, empty vault, focus and 320/390/1440px layouts.
- `cargo +1.91 test --locked`: passed; Stripe live test ignored. Database-dependent
  Rust tests can return early without a configured test DB, so this is not DB coverage.
- `cargo +1.91 fmt --check` and `cargo +1.91 clippy --all-targets -- -D warnings`: passed.
- Changed JS/TS/CSS files formatted with Prettier; `git diff --check`: passed.
- Desktop/mobile screenshots inspected; no overlap or page-wide overflow.

## Deferred / limits

- Full Postgres-backed browser/API suite: no dedicated test database configured;
  local PostgreSQL server binaries are absent and Docker access is denied. No
  production fallback was attempted. Deno runtime fixture was not run (Deno absent).
- Identity signing, Lit execution and provider calls in browser smoke are simulated.
  Tests do not claim live passkey/wallet/Google, TEE or provider authorization.
- Existing per-secret/SDK mutation defaults can still renew permissions to 30 days;
  changing their semantics globally is outside this focused onboarding change.
- Multi-secret approvals remain separate signed transactions, not an atomic batch.
  Concurrent changes still rely on existing revision/conflict checks. Access labels
  in the new form are per-secret, not a new vault-wide agent registry.
- Existing dependency deprecation notices, Rollup annotation warnings and large
  bundle-size warning remain; dependency/code-splitting work is separate.
- No push, PR, deployment or npm publication. Installed copies of the skill and
  already published SDK error text will not update until separately distributed.
