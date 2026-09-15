# `lit-frost-wasm` — the FROST WASM build

A `wasm-bindgen` wrapper that exposes Lit's audited FROST crates to JavaScript,
fixed to the **Ed25519** ciphersuite (Solana). It is the FROST analogue of the
published `@silencelaboratories/dkls-wasm-ll-*` package the ECDSA example uses:
the *same* compiled wasm runs both inside the Lit Action (web build) and on the
user's machine (node build).

- **`frost-dkg`** ([mikelodder7/frost-dkg](https://github.com/mikelodder7/frost-dkg)) — real distributed key generation, group `vsss_rs::curve25519::WrappedEdwards`. No trusted dealer: the full key never exists in one place.
- **`lit-frost`** ([LIT-Protocol/lit-frost](https://github.com/LIT-Protocol/lit-frost), Kudelski-audited) — the 2-round FROST signing protocol + aggregation, `Scheme::Ed25519Sha512`.

## Exposed API (see `src/lib.rs`)

| Function | Used by | Returns |
| --- | --- | --- |
| `dkg_round1(my_id, all_ids, threshold)` | keygen | `{ state, out: [{dst,data}] }` |
| `dkg_round2(state, incoming)` | keygen | `{ state, out }` |
| `dkg_round3(state, incoming)` | keygen | `{ signing_share, verifying_key, verifying_share, solana_pubkey }` |
| `sign_round1(signing_share)` | sign | `{ nonce, commitment, verifying_share }` |
| `sign_round2(message, my_id, signing_share, verifying_key, threshold, commitments, nonce)` | sign | `{ signature_share, verifying_share }` |
| `aggregate(message, verifying_key, commitments, signature_shares, verifying_shares)` | sign | `Uint8Array` (64-byte Ed25519 sig) |
| `verify(message, verifying_key, signature)` / `verifying_share(signing_share)` | sign / recovery | `bool` / `Uint8Array` |

All non-trivial values cross the boundary as bytes: lit-frost types as their
`serde_bare` encoding, the finished signature as the raw 64 bytes (submit-ready
for Solana), and the group key also as raw 32 bytes (`solana_pubkey`, = the
Solana address — base58-encode on the JS side).

## Build

```bash
./build.sh        # produces pkg-web/ (action) and pkg-node/ (client)
```

## Two prerequisites before this compiles + runs

These were uncovered while designing the wrapper and are the remaining work to
make the example run end-to-end. Until they're resolved, `build.sh` will not
produce working packages.

### 1. `frost-dkg::Participant` needs serde — ✅ patch captured + verified

The Lit Action is **stateless across HTTP calls**, so its DKG participant must
serialize between rounds (the action seals the `state` blob to its PKP and
replays it next round — same model as the ECDSA example seals `session.toBytes()`).
The DKG needs exactly one such checkpoint (round 1 → round 2; round 3 has no
outgoing messages and rides along in the second call).

Upstream `frost_dkg::Participant<I, G>` (0.5.1) derives only `Clone`, and its
fields are `pub(crate)`. Every field's *type* is already serde-capable, so the
fix is a small in-crate patch: derive `Serialize, Deserialize` on `Participant`
and annotate its raw group/scalar fields with the `group` / `prime_field` /
`prime_field_vec` serde helpers (wrapped fields get explicit `#[serde(bound)]`),
exactly mirroring how `data.rs` already annotates `Round1Data`. (`Round` and the
`*ParticipantImpl` structs already derive serde.)

**This is captured in [`../frost-dkg-serde.patch`](../frost-dkg-serde.patch)** and
**verified**: with it applied to a vendored frost-dkg, the whole wrapper crate
compiles (`cargo check`, 0 errors). `frost-dkg` is Lit-authored (mikelodder7), so
upstream it as a PR / `serde` feature; until it lands on crates.io, point
`Cargo.toml` at a fork branch carrying the patch (commented stanza there). The
local validation used a gitignored `./.vendor/frost-dkg` + a `[patch.crates-io]`
override; to reproduce: copy the crate there, `git apply` the patch, `cargo check`.

> Signing needs **no** patch — its only inter-round secret is lit-frost's
> `SigningNonces`, which already serializes.

### 2. wasm curve-stack bring-up

The dependency tree must compile to `wasm32-unknown-unknown`. Status:

- **Native build: ✅ clean** (`cargo check`, 0 errors) once `multiexp` is pinned
  to `=0.4.0` — a fresh resolve grabs 0.4.2, whose `multiexp<G: Zeroize>` bound
  breaks `elliptic-curve-tools 0.1.2` (E0277 `DefaultIsZeroes`). Upstream lockfiles
  pin 0.4.0; the pin is in `Cargo.toml`.
- `frost-dkg` is pulled with `default-features = false, features = ["curve25519"]`
  to drop `blst` and the curves we don't use (`ed448`, `jubjub`, `p384`, …).
- `getrandom` is pinned with the `js` feature so `OsRng` works in the browser/Deno
  build (frost-dkg's `new_secret` and lit-frost's `signing_round1` both use it).
- **wasm32 target: ✅ compiles** (`cargo check --target wasm32-unknown-unknown`,
  0 errors) after dropping the BLS stack. `lit-frost` depends on `lit-rust-crypto`
  *without* `default-features = false`, so its `default` feature (`blsful/default`
  + `blst`) pulled in `blsful → blstrs_plus → blst` — a C lib that won't
  cross-compile (cc-rs: "No available targets… for triple wasm32-unknown-unknown").
  `lit-frost`'s source uses no BLS, so the fix is `default-features = false` on
  that dependency (+ drop the explicit `"blst"`), keeping only the curve features
  its code needs. Captured in [`../lit-frost-no-blst.patch`](../lit-frost-no-blst.patch);
  upstream as: make `blst` optional in `lit-frost`.

**All three prerequisites are resolved and verified** — the wrapper compiles to
both native and `wasm32-unknown-unknown` with the two captured patches applied
(via gitignored `./.vendor` + `[patch.crates-io]`). Remaining: run `build.sh` to
emit `pkg-web`/`pkg-node`, then wire `action/` + `client/` to them.

## Wiring it in

Once `pkg-web` / `pkg-node` exist:

- **Client** (`client/mpcClient.js`): `require("../wasm/pkg-node/lit_frost_wasm.js")`.
- **Action** (`action/mpcSigner.js`): publish `pkg-web` to npm + pin a jsDelivr
  URL (as the ECDSA action imports DKLs), or — for max trust — inline the `.wasm`
  as base64 so the action CID commits to the exact crypto bytes.
