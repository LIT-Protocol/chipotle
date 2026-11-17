# AMD deployment attestation

The prod16 deployment uses AMD SEV-SNP. Its guest agent rejects `/GetQuote`
with `GetQuote is Intel TDX only, use Attest on this platform`.

## API and verification

- Upgrade the Rust `dstack-sdk` and `dstack-sdk-types` dependencies to 0.1.3.
  Keep the existing unversioned `/GetKey` derivation and its paths/purposes.
- `/attestation/unified` returns the SDK's hex-encoded versioned attestation
  bundle from `/Attest`, with 64 zero report-data bytes. This endpoint is for
  deployment verification, not a nonce-bound freshness protocol.
- Pass the bundle unchanged to the existing digest-pinned dstack-verifier 0.6.1.
  Require `is_valid=true`; do not substitute health checks or skip verification.
- `/info` preserves the guest's platform-specific data, including null TDX
  measurement fields, while retaining the decoded `tcb_info` object shape.
- `/attestation` and `/attestation/v2` retain their existing TDX contracts.
  They are not AMD-compatible. Consumers verifying AMD replicas must support
  the unified evidence format; the v2 nonce/signature protocol has not been
  migrated by this deployment fix. Audit external consumers before production.

## Test accounts

Staging smoke/correctness workflows explicitly select `accounts.next.json` on
both main and feature-branch dispatches. The previous branch-based default chose
dev accounts for a next deployment, resulting in 401 responses. A read-only
billing-balance probe returned 401 for a dev fixture and 200 for a next fixture;
no credential rotation or key-derivation change is necessary for that mismatch.

## Acceptance

Deploy the new API to the stopped next replica. Require direct endpoint identity
and commit checks, unified attestation verification, OpenAPI, smoke, correctness,
soak and public cutover checks. Then verify one normal deployment in the opposite
direction. Keep the existing rollback gates. Production migration remains pending
until next passes and its attestation consumers have been reviewed.
