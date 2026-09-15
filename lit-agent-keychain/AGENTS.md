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

- No protected plaintext or owner/agent/session private keys in the DB, telemetry,
  API responses or action logs. Master/bootstrap keys never reach clients. Per-vault
  execution-only usage keys are deliberately returned to owners/agents and stored
  encrypted with a separate vault-bound AEAD key. They provide billing, not authority.
- Free is 5 stored secrets; Standard is $10/month for 1,000. Rotations consume no extra slot.
  Custom plans require explicit operator settings. No automatic overage charges.
  Direct Chipotle usage has no hard per-user spending cap (accepted launch limit).
- Owner verification is pinned in an immutable authorization action. Receipts bind
  exact canonical objects. Secret actions pin that authority CID and release mode.
- The DB selects policy freshness. Operator rollback to old valid permissions is
  explicitly accepted; signatures/revision counters do not prevent it.
- Browser/SDK public-key bootstrap goes directly to a configured trusted Lit origin.
  Never trust a Keychain-supplied replacement public key or Lit endpoint.
- Keep all executable action dependencies in the bundle. Never fetch/import mutable
  executable code. Never expose an arbitrary signing, key-export or decryption op.
- "Use inside Lit" actions come from the pinned `@lit-protocol/agent-keychain-library`
  package (public repo LIT-Protocol/agent-keychain-library; `actions/<id>/` manifest +
  code) and have no export path, even for the owner. The harness in
  `actions/secret-common.ts` enforces the manifest: credential pattern, HTTPS host
  allowlist, input/output shapes, request budget and size caps. Action code may import
  only the library's `lib.ts`. Never reflect upstream strings/errors. Bump the pin with
  `npm run library:pin <sha>` (tarball URL + integrity in the lockfile; CI needs no git
  or npm credentials), then `--update-lock` and review that only new hashes appear.
- `actions/catalog.lock.json` pins every template's SHA-256 (authority included).
  `npm run build:actions` fails on drift; pass `--update-lock` only for an intended,
  reviewed release. Never delete a catalog id; set `deprecated: true` instead.
- No operator grants, setup bearer authority, managed PKP vaults, or permissive
  default agents. New secrets start with an empty allowlist.
- Owner sign-in proofs expire; owner credential membership is independent and can
  be indefinite. Agent access policies always have finite signed expiry.
- Every successful mutation commits its audit entry in the same transaction.
  Bootstrap execution counters are atomic and enforced before sponsored login calls.
- Preserve deployed action releases byte for byte: `actions/archive/` is append-only
  and every version is compiled into the server. Source/dependency changes alter
  CIDs/keys and are an explicit `--update-lock` release; the archive, per-vault
  authority records and the authority action's root-owner fallback (SECURITY.md,
  "Authority releases") are what let existing vaults and secrets keep working. Never
  delete or edit an archived template.

## Required validation

`npm run build`, `npm test`, browser tests, Rust tests, `cargo fmt --check`, and
`cargo clippy --all-targets -- -D warnings`. Run Postgres integration and the actual
Deno runtime fixture when changing authorization/encryption. Document which platform
boundaries were simulated; do not claim a live TEE test from a local key fixture.
