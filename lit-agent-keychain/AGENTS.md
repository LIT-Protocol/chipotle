# Agent Context: lit-agent-keychain v2

Rust storage/sponsorship API plus a React client, agent SDK and bundled Lit Actions.
See README.md and SECURITY.md for the protocol and its explicit trust boundary.

## Tools

- Rust 1.91: run Cargo here, never from the monorepo root.
- JS/TS: `npm ci`, `npm run build`, `npm test` here. This is its own npm workspace;
  the unrelated repo e2e suite still uses pnpm.
- Run `npm run build:actions` before Cargo: Rust includes the exact generated action
  templates. The browser and Rust must generate byte-identical sources/CIDs.

## Invariants

- No protected plaintext, owner/agent/session private keys, or Lit execution key
  in the database, telemetry, API responses or action logs. Ciphertexts, signatures,
  public keys and metadata are intentionally stored.
- Owner verification is pinned in an immutable authorization action. Receipts bind
  exact canonical objects. Secret actions pin that authority CID and release mode.
- The DB selects policy freshness. Operator rollback to old valid permissions is
  explicitly accepted; signatures/revision counters do not prevent it.
- Browser/SDK public-key bootstrap goes directly to a configured trusted Lit origin.
  Never trust a Keychain-supplied replacement public key or Lit endpoint.
- Keep all executable action dependencies in the bundle. Never fetch/import mutable
  executable code. Never expose an arbitrary signing, key-export or decryption op.
- The Stripe-only action has no export path, even for the owner. Its destination,
  method and projected response are fixed. Never reflect upstream strings/errors.
- No operator grants, setup bearer authority, managed PKP vaults, or permissive
  default agents. New secrets start with an empty allowlist.
- Owner sign-in proofs expire; owner credential membership is independent and can
  be indefinite. Agent access policies always have finite signed expiry.
- Every successful mutation commits its audit entry in the same transaction.
  Execution counters are atomic and enforced before sponsored calls.
- Preserve deployed action releases byte for byte. Source/dependency changes alter
  CIDs/keys and require an explicit new release and owner-controlled transition.

## Required validation

`npm run build`, `npm test`, browser tests, Rust tests, `cargo fmt --check`, and
`cargo clippy --all-targets -- -D warnings`. Run Postgres integration and the actual
Deno runtime fixture when changing authorization/encryption. Document which platform
boundaries were simulated; do not claim a live TEE test from a local key fixture.
