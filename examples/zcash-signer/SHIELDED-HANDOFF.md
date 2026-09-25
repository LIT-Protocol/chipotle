# Zcash signer — handoff / resume notes

This PR (draft) ships the **transparent** Zcash signer example. The original
intent was to pivot it to a **shielded (Orchard)** signer; that work was scoped,
proven feasible with a real spike, and then parked behind two infra limit
bumps. This doc captures everything needed to pick it back up.

---

## 1. What's in this PR right now (transparent — done & verified)

`examples/zcash-signer/` — a keyless Zcash **transparent** (`t1`) wallet bound
to a Lit Action's CID, modeled on `examples/solana-signer`.

- The action derives a **secp256k1** `t1` address from its own identity key
  (`getLitActionPrivateKey()`), builds a **v4 (Sapling) transaction**, computes
  the **ZIP-243 BLAKE2b sighash** (personalized with the NU6 consensus branch
  ID `0xc8e71055`), signs each input, and returns raw tx hex. Policy: single
  recipient `t1`, amount under a code-bound cap, change forced back to itself,
  fee capped.
- Client (`scripts/`) uses **Blockchair mainnet REST** for UTXOs / tip height /
  broadcast. It targets **mainnet** because Zcash *testnet* REST infra is dead
  (no public testnet Blockbook, Blockchair has no testnet, `explorer.testnet.z.cash`
  is gone).
- **Verification done:** the ZIP-243 sighash machinery was checked against all
  10 official `zcash-test-vectors` (`zip_0243.json`) — 10/10 pass, including the
  transparent-input cases — using the action's exact helper code. Address
  derivation, low-S DER signing, and tx assembly were checked against the
  canonical secp256k1 `privkey=1` vector and an end-to-end run of the real
  action file (npm-shimmed imports, mocked `Lit.Actions`).
- **Gotcha already fixed:** `@noble/secp256k1` v2 **dropped DER** (`toDERRawBytes`
  / `fromDER` don't exist). The action DER-encodes `(r,s)` itself from the
  compact signature. Don't "simplify" that back to `.toDERRawBytes()`.

If the shielded version supersedes this, the transparent example can be deleted
(the user indicated a preference for shielded). Until then it stands alone.

---

## 2. The shielded pivot — target design

Goal: a keyless **Orchard** (shielded) wallet where the **full Orchard spend
key never leaves the TEE**. Because Orchard separates proving from spend
authorization, the cleanest design (assuming proving fits the runtime — it does,
see §4) is **the action does everything**:

```
Action (Rust→WASM):  derive Orchard spend key from CID identity → build bundle →
                     Halo2 create_proof → RedPallas sign → serialize v5 tx → hex
                     (also exposes its unified address + FVK)
Client (Node):       scan with the FVK via lightwalletd (testnet) to find the
                     spendable note(s) + Merkle witness + anchor, hand them to
                     the action, then broadcast the returned tx
```

Network: **testnet** via lightwalletd `lwd.testnet.zec.pro` (gRPC) — unlike the
transparent REST situation, shielded testnet works via lightwalletd
(`GetAddressUtxos`-style scan + `SendTransaction`) with free TAZ.

### Curve / crypto facts (why secp256k1 doesn't apply)
- Orchard: **Pallas/Vesta** curves, **RedPallas** spend-auth sigs, **Halo 2**
  proofs (no trusted setup, no params blob). Circuit `k = 11` (2048 rows).
- Sapling (the other shielded pool): Jubjub / RedJubjub / Groth16 (needs ~48 MB
  params) — **not** chosen; Orchard is lighter to ship and the modern pool.
- `@noble/curves/pasta` exports `pallas`/`vesta`, so RedPallas could be done in
  JS if ever needed, but proving must be Rust→WASM regardless.

### Orchard crate API (v0.13) — the two-phase auth is built in
`Builder::new(BundleType::DEFAULT, anchor)` → `add_spend(fvk, note, merkle_path)`
/ `add_output(ovk, addr, NoteValue, memo)` → `build()` →
`create_proof(&pk, rng)` (**takes no `ask`** — proving needs only the FVK) →
`prepare(rng, sighash)` → `sign(rng, &ask)` *or* `append_signatures(&[redpallas::Signature])`
(external signer) → `finalize()`. So if you ever want the spend key fully
separate from the prover, the crate already supports it.

---

## 3. v5 transaction + sighash references
- **ZIP-225** — v5 transaction format (header `0x80000005`, versionGroupId
  `0x26A7270A`, `nConsensusBranchId` serialized in the body, Sapling+Orchard
  bundles).
- **ZIP-244** — v5 sighash (a tree of BLAKE2b hashes; `ZcashTxHash_` root
  personalization + branch ID LE; transparent S.2 sub-hashes). Needed if the tx
  has any transparent ins/outs; a pure-Orchard tx still needs the v5 txid/sighash
  tree.
- Consensus branch ID (current, NU6): **`0xc8e71055`**, appended **little-endian**
  in personalizations. Bump at each network upgrade.
- The `orchard` crate handles the Orchard bundle's own sighash internally; the
  outer v5 tx assembly (wrapping the bundle + any transparent parts + computing
  the tx sighash the bundle is `prepare()`d with) is what you build around it —
  `zcash_primitives` / `zcash_protocol` have the v5 tx builder if you don't want
  to hand-roll ZIP-225/244.

---

## 4. Spike: proven feasible, with real numbers

A spike compiled the `orchard` crate (with `circuit`) to `wasm32-unknown-unknown`
and generated a real Halo2 proof (derive key → shielding bundle → `create_proof`
→ `prepare` → `finalize`). It ran **natively, in local Node WASM, and in a real
Lit Action on Chipotle.**

| Step | Native | Local WASM (1-thread) | **Real TEE (Chipotle)** |
| --- | --- | --- | --- |
| `ProvingKey::build()` | 1.3 s | 9.4 s | **23.7 s** |
| full proof (incl. PK build) | ~1.8 s | ~22 s | **~56–58 s** (scaled) |
| peak WASM linear memory | (131 MB RSS) | **~100 MB** (32 MB after PK build → 100 MB after proof) | matched local byte-for-byte at the 32 MB checkpoint |
| wasm binary | — | **2.3 MB** | — |

Conclusions:
- **A Lit Action can generate the Orchard proof.** Memory peak ~**100 MB**, time
  ~**56 s** in the TEE (CPU ~2.5× slower than a laptop). Not the 4 GB worst case
  — that only applies to large multi-action bundles.
- ~**24 s of every call is just rebuilding the proving key** (stateless action
  rebuilds it each time). Optimization later: ship a serialized PK, or use WASM
  threads (rayon) — but threads need nightly + shared-memory/atomics support in
  the TEE.

The spike lives in **`.context/orchard-spike/`** (gitignored — **NOT in this PR**,
will be lost if the workspace is cleaned). Its source is embedded below so it's
recoverable. The spike only builds a *shielding-output* bundle (dummy spends
against `Anchor::empty_tree()`); the real example needs **real spends** (note +
witness + anchor) and **v5 tx assembly** around the bundle.

<details>
<summary><code>orchard-spike/Cargo.toml</code></summary>

```toml
[package]
name = "orchard-spike"
version = "0.0.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]
path = "src/lib.rs"

[dependencies]
orchard = { version = "0.13", default-features = false, features = ["circuit"] }
rand = "0.8"
wasm-bindgen = "0.2"
getrandom = { version = "0.2", features = ["js"] }

[profile.release]
opt-level = 3
```
Build: `wasm-pack build --release --target web` (or `--target nodejs` for local
timing). `default-features = false` drops `multicore`/rayon → single-threaded,
builds on stable. `getrandom` `js` feature is required for `wasm32`.
</details>

<details>
<summary><code>orchard-spike/src/lib.rs</code> (the working prover spike)</summary>

```rust
use orchard::{
    builder::{Builder, BundleType},
    bundle::{Authorized, Bundle},
    circuit::ProvingKey,
    keys::{FullViewingKey, Scope, SpendingKey},
    value::NoteValue,
    Anchor,
};
use rand::rngs::OsRng;
use wasm_bindgen::prelude::*;

// Current wasm linear memory high-water mark in bytes (wasm memory only grows).
#[wasm_bindgen]
pub fn wasm_mem_bytes() -> u32 {
    #[cfg(target_arch = "wasm32")]
    { (core::arch::wasm32::memory_size(0) as u32).saturating_mul(65536) }
    #[cfg(not(target_arch = "wasm32"))]
    { 0 }
}

#[wasm_bindgen]
pub fn prove_shielding(seed_hex: &str, value: u64) -> String {
    let mut rng = OsRng;
    let mut seed = [0u8; 32];
    let bytes = hex_to_bytes(seed_hex);
    seed.copy_from_slice(&bytes[..32]);

    let sk = SpendingKey::from_bytes(seed).unwrap();
    let fvk = FullViewingKey::from(&sk);
    let recipient = fvk.address_at(0u32, Scope::External);

    let pk = ProvingKey::build();

    let anchor: Anchor = Anchor::empty_tree();
    let mut builder = Builder::new(BundleType::DEFAULT, anchor);
    builder.add_output(None, recipient, NoteValue::from_raw(value), [0u8; 512]).unwrap();

    let unproven = builder.build::<i64>(&mut rng).unwrap().unwrap().0;
    let bundle: Bundle<Authorized, i64> = unproven
        .create_proof(&pk, &mut rng).unwrap()
        .prepare(rng, [0u8; 32])
        .finalize().unwrap();

    format!("ok value_balance={}", bundle.value_balance())
}

fn hex_to_bytes(s: &str) -> Vec<u8> {
    let s = s.trim_start_matches("0x");
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i+2], 16).unwrap()).collect()
}
```
</details>

---

## 5. wasm delivery tension (resolve before building the real action)

- Inlining the 2.3 MB wasm as base64 makes a ~3 MB action, which the
  `/lit_action` **HTTP gateway rejects with 413** (request-body cap is well
  below the 16 MB *code* limit). So inline is out for a wasm this size.
- **Fetching the wasm at runtime works** (verified in the TEE: jsDelivr 141 ms;
  an arbitrary host `tmpfiles.org` served the full 2.4 MB in 1.6 s — TEE `fetch`
  is not host-restricted). This is the pattern `examples/mpc-signing-ecdsa` uses.
- **Binding caveat:** fetching means the wasm is **not** committed to the action
  CID, weakening the "key bound to exact code" story. Mitigation: host the wasm
  immutably (npm→jsDelivr, pinned version) **and have the action verify a
  hardcoded SHA-256 of the fetched bytes before instantiating** — then the hash
  (and thus the prover) *is* part of the CID. Do this.

---

## 6. Infra blockers + status (both needed before a full proof returns)

The PK-build step alone (23.7 s, 32 MB) ran fine in the TEE. The **full** proof
hits two limits, both ours, both confirmed:

1. **Ingress timeout (was a 504 at 60 s).** `dstack-ingress` runs nginx
   internally (TLS termination → `lit-api-server:8000`) with the stock 60 s
   `proxy_read_timeout`. **NOT** a Phala/dstack platform limit (dstack-gateway is
   a Rust TCP proxy, idle 10 m / total 5 h, never emits an nginx 504).
   → **Fixed in PR #442** (separate PR against `main`): sets
   `PROXY_READ_TIMEOUT`/`PROXY_SEND_TIMEOUT` to `900s` on the `dstack-ingress`
   service in `docker-compose.phala.yml` (matches the runtime's 15-min
   `DEFAULT_TIMEOUT_MS`). The pinned image already supports those env vars.
   **Needs:** merge #442 + redeploy ingress on **dev**.
   Caveat: dstack-gateway idle = 10 m, so a >10-min *idle* response would still
   be cut — irrelevant at ~56 s.

2. **Action memory limit: 64 MB default < ~100 MB needed.** This is **on-chain**
   config (`LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB`, read from the account-config
   contract `nodeConfigurationValues()` every 30 s; code fallback default 64,
   hard max **640** = `DEFAULT_MEMORY_LIMIT_MB × 10`). 256 is already within the
   allowed max, so **no code change / PR is needed** — set it on-chain via the
   monitor dapp (`lit-static/dapps/monitor/`, "Set Configuration" =
   `setNodeConfiguration`):
   > key `LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB` → value `256`
   **Needs:** set this on **dev**. (A code PR would only be needed to raise the
   fallback default or the 640 ceiling — neither required for 256.)

Other runtime limits are already fine: execution timeout **15 min**, code size
16 MB, response 1 MB (the v5 tx hex should be well under 1 MB — verify).

**Verify limits live:** `GET /core/v1/get_lit_action_client_config` with an API
key returns the effective `memory_limit_mb`, `timeout_ms`, etc. (At time of
writing on prod: `memory_limit_mb: 64`, `timeout_ms: 900000`.)

---

## 7. Resume checklist

1. Land the limits on **dev**: merge/deploy **PR #442** (ingress 900 s) and set
   on-chain `LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB = 256`. Confirm via
   `GET /get_lit_action_client_config`.
2. Re-run the spike's full proof in a real action to confirm it now **returns**
   (was ~56 s / ~100 MB; previously 504'd at the 60 s ingress cap). Harness
   pattern: fetch wasm from a URL, `initSync(bytes)`, call `prove_shielding`,
   return timing + `wasm_mem_bytes()`.
3. Extend the spike crate into the real prover: **real spends** (note + Merkle
   witness + anchor via `add_spend`), recipient + change outputs, and **v5 tx
   assembly** (ZIP-225/244) around the Orchard bundle. Derive address + FVK from
   the CID seed; expose an `address`/`viewing-key` op.
4. Decide + implement **wasm delivery** (§5): host immutably + verify a pinned
   SHA-256 in the action so the prover is bound to the CID.
5. Build the **JS action wrapper** (load wasm, route ops, return tx hex), mirror
   `examples/mpc-signing-ecdsa/action/` for the loading pattern.
6. Build the **client**: lightwalletd gRPC (`lwd.testnet.zec.pro`) — scan with
   FVK for spendable notes + witnesses + a recent anchor, call the action,
   `SendTransaction` to broadcast. This is the fiddliest part; WebZjs
   (`@chainsafe/webzjs-*`) is the reference for client-side scanning.
7. Remove the transparent example (or keep both, TBD) and update
   `examples/README.md`.

## Key references
- ZIPs: [225 (v5 format)](https://zips.z.cash/zip-0225),
  [244 (v5 sighash)](https://zips.z.cash/zip-0244),
  [243 (v4 sighash, used by the transparent example)](https://zips.z.cash/zip-0243).
- `orchard` crate builder (two-phase auth): `src/builder.rs` — `create_proof`
  (no `ask`), `prepare`, `append_signatures`, `finalize`.
- WASM-in-action loading + limits: `docs/lit-actions/wasm.mdx`,
  `docs/lit-actions/limits.mdx`, `examples/mpc-signing-ecdsa/`.
- Limits in code: `lit-api-server/src/actions/client/mod.rs`
  (`DEFAULT_MEMORY_LIMIT_MB`, `MAX_MEMORY_LIMIT_MB`, timeouts),
  `lit-api-server/src/accounts/chain_config.rs` (`ConfigKeys`, on-chain source).
- Test vectors: `zcash-test-vectors` repo (`zip_0243.json`, `zip_0244` for v5).
