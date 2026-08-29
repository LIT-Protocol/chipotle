# Agent Context: lit-secrets (Rust)

## Purpose
Programmable credential access control plane on top of Chipotle. Users sign in
with magic links, store secrets (sealed to a per-tenant vault PKP by running the
encrypt action on Chipotle), mint scoped agent usage keys, and set per-secret
policy. Agents call `POST /api/grants` for a signed grant and redeem it against
Chipotle's `lit_action` with the pinned reader action; plaintext never touches
this service. Key modules: `tenants.rs` (provisioning), `secrets.rs`,
`agents.rs` (usage-key guard), `grants.rs`, `policy.rs`, `signer.rs`,
`actions.rs` (+ `actions/*.js`), `chipotle.rs`, `audit.rs`, `auth/`.

## Stack & Tooling
- Toolchain: Rust 1.91
- Key libraries: Tokio, Rocket 0.5, sqlx (Postgres, no ORM, plain `.sql` migrations),
  k256 + alloy-primitives (EIP-191 grant signing), ipfs-hasher (action CIDs)
- Linting: `cargo clippy --all-targets -- -D warnings`

## Invariants — do not break
- No plaintext secret value is ever persisted, logged, or returned by this
  service. Only Chipotle returns plaintext, and only to the agent.
- `actions/reader.js` + `GRANT_SIGNING_KEY` ⇒ reader CID. Changing either
  changes the CID and orphans existing tenant groups; bump deliberately.
- `Grant` field order in `grants.rs` is the signed canonical form the reader
  verifies verbatim. Never reorder/rename fields without a `v` bump.
- Agent usage keys are stored only as sha256 hash + AES-GCM ciphertext.

## Coding Rules
- No `.unwrap()`/`.expect()` on request paths; propagate with `?` / `ApiError`.
- Chipotle failures surface as 502 with the upstream message (`api::upstream`).
- Every grant/reference decision, allow or deny, is written to `access_log`.

## Definition of Done
1. `cargo clippy --all-targets -- -D warnings` clean.
2. `cargo fmt --check` clean.
3. `cargo test` passes.
