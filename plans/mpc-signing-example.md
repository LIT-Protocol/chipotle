# `mpc-signing` — a new `examples/` entry

**Status:** ✅ Built and verified end-to-end on production. Full flow run against the live Lit network + Base Sepolia: `setup` → `keygen` (5-round DKG) → `sign --dry` → `deploy` (`MpcVault`) → on-chain `exec` (contract `ecrecover` accepted the 2-of-2 signature). All previously-unverified pieces confirmed: action's jsDelivr import + runtime wasm fetch, `Lit.Actions.Encrypt`/`Decrypt`, `CompressionStream` gzip, the per-round relay, and the 16 MB responses (prod limit raised from the default 100 KB).

**2-of-3 + cold-storage recovery added** (`keygen --with-recovery`, `sign --recovery`; cold share stored separately and moved offline; recovery signs hot+cold entirely client-side with no Lit). The user runs two parties (hot=0, cold=2), Lit runs party 1. Correct + reliable locally (15/15 in a prod-faithful Deno harness; all three quorums {hot,Lit}/{hot,cold}/{Lit,cold} recover the same address). On prod it has completed but the **multi-party DKG intermittently fails — and retry is not reliable** (sustained 100%-failure windows observed). Two distinct symptoms, both thrown from inside the wasm during `handleMessages`: **"Missing message"** (round 2, peer-message array) and **"Invalid commitment hash"** (round 4, commitment array). 2-of-2 (single peer per round, no commitment-array round in the same way) is fully reliable incl. on-chain. Offline the 2-of-3 path is bulletproof: **30/30 real client, 15/15 sequential, 64/64 concurrent** in a prod-faithful Deno harness (same web wasm build, gzip+relay, routing). Tried and ruled out as fixes: client bug (no), wasm-instance reuse (no), concurrency/heap reentrancy (no), a "touch each incoming message before handleMessages" probe (helped round-2 in some runs but a different symptom appeared and it's not reliable — removed), and aggressive retry with backoff (rides over short windows, fails on sustained ones). **Conclusion: node-side runtime bug** in the `lit_actions` worker's execution / wasm-object handling for the multi-peer (n>2) DKG rounds — needs infra investigation, not an example-code change. `wasm-demo/` is a minimal repro of the working path; the failure only appears on the live worker. The recovery (`--recovery`) sign path is fully local and unaffected.

### Node-side investigation (deno version angle)

- **Version mismatch found:** the node embeds **Deno 2.2.2** (`deno_core 0.338.0`, V8 13.4.114.9-rusty); my earlier "prod-faithful" repros ran on the local CLI **1.44.4** (V8 12.6) — the wrong engine.
- **Tested the node's exact V8 and the upgrade target:** downloaded Deno **2.2.2** (V8 13.4) and **2.8.1** (V8 14.9, the PR #388 era). Ran the relayed 2-of-3 keygen — sequential, concurrent, and **GC-stressed with forced `globalThis.gc()` at every op-yield point** — **60/60 pass on both.** So it is **not** a stock-V8 / Deno-CLI bug.
- **Workers are fresh-isolate-per-request** (`server/worker_pool.rs`: each worker runs one request then is dropped) → no cross-request heap contamination. The host caps each isolate at **64 MB heap**, runs V8 with `--memory-protection-keys` (PKU) + `--clear-free-memory`, and the action **re-fetches + re-compiles the 642 KB wasm every round**.
- **Prime structural suspect:** the action's `Encrypt`/`Decrypt` are `#[op2(async, reentrant)]` gRPC ops (`ext/bindings.rs` → `remote_op_async!`). The node already carries a `deno_core` patch on branch **`fix/deno-222-op-panic`** (upstream `denoland/deno_core#730`) for *exactly* "reentrant gRPC op fails + `isolate.terminate_execution()` → panic." This op+termination-under-load path on 2.2.2 is the known-fragile area. (Op *count* is identical for 2-of-2 and 2-of-3, so the trigger is the interaction with the larger multi-message rounds under real load, not the ops alone.)
- **Ran the real `lit-actions` runtime in-process** (the `tests/it.rs` gRPC harness — exact deno_core 2.2.2 embedding, snapshot, ops, 64 MB limit, import pipeline). Captured the action party's exact inputs from an offline 2-of-3 keygen (session bytes + the two peer messages per round) and replayed the failing `handleMessages` in the real runtime:
  - **Sequential, every round (r2–r5): clean.**
  - **Concurrent up to 384 in-flight: clean** (only transient failures: `"Failed to bundle CDN imports: failed to fetch jsdelivr"` — the bundler's per-request glue fetch under load; a *separate* flakiness source, fixable by inlining the glue, **not** the "Missing message" bug).
  - **Concurrent + reentrant `Decrypt`/`Encrypt` op yields around `handleMessages`: clean (192/192).**
- **Tested the real prod Encrypt/Decrypt round-trip** (332 KB payloads, 24 concurrent × 5): **0 mismatches** — the relay crypto is not lossy under load.
- **Net: every isolatable component is clean** — example code, stock V8 (2.2.2 & 2.8.1), single-call wasm `handleMessages` (even concurrent), reentrant gRPC op yields, and crypto round-trip. The failure only manifests in the **full chained 5-execution keygen relay on the live network**, which the `lit-actions` harness can't reproduce because it mocks Encrypt/Decrypt (so the real cross-execution relay never runs). Reproducing it locally needs the **full stack** (`lit-api-server` → `lit-actions` + real key management), driven by the actual client's chained keygen under concurrency — that adds the `lit-api-server` layer (js_params/response relay, billing) which is the only untested surface.
- **Will PR #388 (Deno 2.8.x) fix it?** Plausible but unconfirmed: 2.8.1 passes all local stress, and the PR "eliminates the need for the deno_core patches" (the op-panic workaround is obviated upstream). **Definitive test:** run `npm run keygen -- --with-recovery` against a node built from PR #388 — this example *is* the reproduction. If it passes reliably there, ship 2-of-3.
- **Also worth fixing regardless:** inline the DKLs glue + wasm in the action (no per-request jsDelivr fetch) — the harness showed the bundler's CDN fetch is itself a load-dependent failure source.

### ✅ ROOT CAUSE FOUND (full local stack)

Ran the entire stack locally (anvil + dstack-simulator + `lit_actions` + `lit-api-server`, provisioned via the `e2e/global-setup.ts` payer recipe + registering/funding all `requestedApiPayerCount` signers) and **reproduced the 2-of-3 keygen failure** (intermittent ~25–50%). Instrumented all layers (client / lit-api-server boundary / action) and found:

1. The client threads `encState` correctly, and lit-api-server forwards the correct js_params — but the **action executes with a *different* request's entire js_params** (the `sessionId` the action receives ≠ the one sent). So js_params are crossed **inside `lit_actions`**.
2. Mechanism: per-request js_params are **baked into the source** (`to_executable_code` → `const params = {…}`) and run via `__litEvalCached` → `op_eval_context` → the **`SharedV8CodeCache`** (server/v8_code_cache.rs), keyed by `(specifier, kind, source_hash)`. The specifier is the constant `file:///user_provided_script.js`.
3. **Deno's `op_eval_context` computes the *same* `source_hash` for *different* sources of the same length.** Verified directly: two requests with different source content (`myhash` 86e200e7 vs 48f21109, both len 38164) produced the identical Deno `source_hash=4decc2ac…` and both HIT — returning a prior request's compiled bytecode (with *its* baked-in js_params). The action then runs the wrong session's `encState` → the wasm legitimately rejects it (`Invalid commitment hash` / `Missing message`). Same root also produces the intermittent **V8 native stack-overflow crash** (`V8_Fatal: Check failed: stack_overflow()`).
4. **Verified fix:** bypass the code cache for user code (run `user_code` via `execute_script` directly, which skips `op_eval_context`/`SharedV8CodeCache`) → **12/12 keygens pass, zero retries, no crash.**

Why nothing reproduced it before: the failure needs (a) the Lit-specific `__litEvalCached`/`SharedV8CodeCache` eval path (absent from the plain `deno` CLI and from the `lit-actions` gRPC test harness's typical single-shot usage) and (b) a stream of same-length-but-different-content sources to trigger Deno's `source_hash` collision — exactly what the chained keygen with large, similar-sized relayed sessions produces.

**Fix options (node-side, not the example):**
- **Best:** stop baking per-request params into the code-cached source. Inject js_params as a runtime global (the `inject_params_globals` hook, currently `None`) so the source that flows through `op_eval_context` is the stable, param-free bundled action code — then `source_hash` collisions are harmless (the bytecode genuinely matches).
- **Or:** give the eval-context cache a content-based `source_hash` (hash the full source) instead of relying on Deno's colliding hash.
- **Or (simplest, verified):** bypass the code cache for user code (`execute_script`), accepting a recompile per request.

**Does PR #388 (Deno 2.8.x) fix it?** Possibly — if 2.8.x's `op_eval_context` uses a content-correct `source_hash`. But the robust fix is Lit-side (don't route param-laden source through the code cache); it shouldn't depend on the Deno hash behavior.

Also corrected during 2-of-3 work: `Lit.Actions.Encrypt` binds to the **PKP**, not the action CID — sealed state survives action edits (verified), and tighter CID-binding requires restricting the group's `cid_hashes_permitted`. README updated.
**Owner:** chris@litprotocol.com
**Target location:** `examples/mpc-signing/`

## One-line pitch

A signing flow where **Lit literally cannot produce a signature on its own.**
The signing key is split between a PKP-bound share held inside the Lit Action
and a share held by the user, and every signature requires both parties to run
an interactive MPC protocol. The full private key never exists anywhere — not
even momentarily inside the action. The output is a standard secp256k1 ECDSA
signature that any EVM contract verifies with plain `ecrecover`.

This fills a gap in the current `examples/` lineup: every existing example
(`compliance-transfer-gate`, `cross-chain-token`, `multi-source-price-oracle`,
`prediction-market-oracle`) is a "Lit signs on behalf of an attestation" flow
where the action can sign whenever it wants. None demonstrate **two-party
signing where the user is a required co-signer.**

---

## Decisions locked in (from the design discussion)

1. **Real MPC, not Shamir.** We considered a tier-1 "reconstruct the key inside
   the action from a Shamir/additive split" version. Dropped it. The whole point
   is that a compromised action can't extract the key, and Shamir reconstructs
   the full key in the V8 isolate at sign time. We do real 2-party ECDSA where
   the key never exists anywhere.

2. **Library: DKLs23 via `@silencelaboratories/dkls-wasm-ll-web` (WASM).**
   Not the pure-JS Lindell17 (`@silencelaboratories/ecdsa-tss`). Two reasons:
   - DKLs23 is general **t-of-n**, so the *same* example/library generalizes to
     **2-of-3 with a cold-storage recovery share** (see below). Lindell17 is
     hard-wired 2-of-2 — a dead end.
   - Deliberately paying the **WASM-in-a-Lit-Action** cost now builds the
     platform we need anyway for the FROST flagship (demo #2), which can only
     ship as WASM (`lit-frost`).
   DKLs23 is Trail-of-Bits audited (Feb 2024). Library is 642 KB wasm + 41 KB JS.

3. **EVM / ECDSA first.** EVM is where Lit's audience and all existing examples
   are, and DKLs23 output is a standard ECDSA sig → `ecrecover` works → drops
   into any contract or holds real ETH at a normal address. FROST/Bitcoin-Solana
   is demo #2 (see "Roadmap").

4. **Interactive is fine.** We explored non-interactive/presigning and decided
   not to require it for v1. Each signature is a multi-round interactive
   protocol; that's acceptable for the demo. (Presigning stays a documented
   future optimization, also relevant to the response-size note below.)

5. **Why not just a Gnosis Safe 2-of-2?** The honest answer the README must give:
   **portability — scoped correctly.** Be careful not to overclaim here. DKLs23
   (ECDSA) and FROST (Schnorr/EdDSA) are different protocols producing
   *cryptographically incompatible* shares, so there is **no single share that
   signs every chain.** What's true:
   - **One DKLs23 secp256k1-ECDSA share controls every ECDSA-secp256k1 chain**
     with no per-chain contract: all EVM chains, plus Bitcoin legacy/SegWit
     (ECDSA), Tron, etc., and via DKLs's built-in HD `chain_path`, many derived
     addresses from that one share. That alone beats deploying a Safe on every
     EVM chain.
   - **Solana (Ed25519) and Bitcoin Taproot (Schnorr) need their own separate
     FROST shares** — same Lit + user model and same device, different key.
   So the accurate pitch is **"one share per signature-scheme family, many chains
   each"** — *not* "one universal share." The claims that always hold: no
   per-chain multisig contract anywhere; on EVM the key is a normal address
   (cheaper, private, no contract); and the same 2-of-2 architecture/UX extends
   to every chain. The cross-family reach lands once demo #2 (FROST) exists.

---

## ✅ De-risk: the hard parts are proven

Before writing the example, the risky mechanics were validated end-to-end in
Deno (the action runtime is Deno v2.2.2; verified on Deno 1.44). Scripts live in
[`examples/mpc-signing/wasm-demo/`](../examples/mpc-signing/wasm-demo/).

What's proven:

- **WASM loads in the action sandbox.** `initSync(base64-decoded bytes)`
  instantiates the 642 KB DKLs wasm with no fetch/URL and no Node APIs — only
  `WebAssembly` + `globalThis.crypto`, both present in the action runtime. The
  library is even authored/tested against Deno (`wasm-pack build -t web`,
  `deno test`).
- **The stateless encrypt-relay pattern works.** A full 2-of-2 DKG and a full
  2-of-2 signing both complete with the "action" party's session serialized via
  `toBytes()` and rebuilt via `fromBytes()` **between every single round** —
  exactly what the action will do (decrypt session → process round → re-encrypt
  → return). The DKLs sessions are serializable by design.
- **`ecrecover` compatibility.** The produced `[R, S]` recovers to the DKG
  public key's EVM address (`v=28` in the test). A normal EVM contract accepts
  it with no special verifier.

### ⚠️ The one real constraint we found: response payload size

The action is stateless, so each round it must return its (encrypted) session
to the user via the **response** — and the default response cap is **100 KB**
(`docs/lit-actions/limits.mdx`). Measured blob sizes (raw / gzipped):

| Phase | raw | gzip | vs 100 KB |
| --- | --- | --- | --- |
| DKG, all rounds | ≤122 KB | ≤79 KB | ✅ fits gzipped |
| Long-lived keyshare (stored by user between sessions) | 121 KB | 78 KB | ✅ fits gzipped |
| **Signing rounds 1–2** | **214 KB** | **138 KB** | ❌ over, even gzipped |

The two middle signing rounds carry the keyshare *plus* the OT/MtA
precomputation. Resolution, both cheap:

1. **gzip every relayed blob** before `Lit.Actions.Encrypt` (halves it; brings
   DKG + keyshare comfortably under, and the action runtime has
   `CompressionStream`).
2. **Raise this example's response limit to ~256 KB.** The cap is a documented,
   configurable default; setup already calls management APIs, so it can request
   the raise for the example's group. README documents the requirement.

(Longer term, DKLs23 presigning moves the heavy OT precompute out of the online
signing path, shrinking the online blobs — a future optimization, not needed to
ship.)

---

## DKLs23 API & round structure (as verified)

From the package `.d.ts` and the authors' Deno test. Classes:
`KeygenSession`, `SignSession`, `Keyshare`, `Message` (all with
`toBytes`/`fromBytes`).

**DKG** (`new KeygenSession(participants, threshold, party_id)` — `(2,2,…)` here):
`createFirstMessage()` → `handleMessages()` ×4 (one carries the chain-code
commitments from `calculateChainCodeCommitment()`) → `keyshare()`.
≈ 5 message exchanges → 5 action round-trips.

**Signing** (`new SignSession(keyshare, chainPath)`): `createFirstMessage()` →
`handleMessages()` ×3 → `lastMessage(msgHash)` → `combine(msgs)` → `[R, S]`.
≈ 5 message exchanges → 5 action round-trips.

`combine()` returns `R` and `S` as 32-byte arrays; the harness derives `v` by
trial recovery against the known public key (standard).

Bonus built-ins relevant to the 2-of-3 roadmap: `initKeyRotation`,
`initKeyRecovery`, `initLostShareRecovery`, and HD derivation via `chainPath`.

---

## Trust model

```
   ┌──────────────────────────────────┐
   │ User's machine (party 0)         │
   │   ├── share_B (Keyshare, local)  │
   │   └── encrypted_action_session   │  ← sealed to THIS action's CID
   └─────────────┬────────────────────┘
                 │ each round: { encrypted_action_session, user_round_msgs }
                 ▼
   ┌──────────────────────────────────┐
   │ Lit Action (this CID, stateless) │
   │  decrypt → SignSession.fromBytes │
   │  check session_id + round        │
   │  handleMessages(...)             │
   │  SignSession.toBytes → gzip →    │
   │     Lit.Actions.Encrypt          │
   │  return { action_msgs, encrypted_action_session }
   └──────────────────────────────────┘
```

- **The full key never exists anywhere**, by construction of DKLs23. share_A
  alone (or a compromised action holding it) cannot produce a signature without
  the user interactively contributing share_B's rounds. ✅
- **Lit can't sign alone**, and a compromised V8 isolate can't extract a
  signable key. ✅
- **CID binding.** The action's session is sealed with
  `Lit.Actions.Encrypt({ pkpId })`, decryptable only by this exact action CID.
  Editing the action mints a new CID and the user's stored blobs stop
  decrypting — so policy is content-addressed, same trust anchor as the other
  examples.
- **Relay integrity.** The encrypted blob carries `session_id + round number`;
  the action rejects replays/round-splices. DKLs's own per-round commitments
  catch fabricated messages.
- **Output** is a standard secp256k1 ECDSA signature — on-chain it's
  indistinguishable from a normal EOA signature.

---

## Generalizing to 2-of-3 with cold storage

This is why we chose DKLs23 over Lindell17. `KeygenSession(3, 2, party_id)`
gives a 2-of-3:

| Share | Held by | Role |
| --- | --- | --- |
| Share 1 | Lit (PKP-bound, in the action) | co-signer |
| Share 2 | User — hot (browser/CLI) | day-to-day |
| Share 3 | User — cold storage | recovery |

- **Normal signing:** Lit + hot = quorum; cold stays offline.
- **Lit can't sign alone; hot alone can't sign.** Same strong property.
- **Self-custody escape hatch:** the user holds 2 of 3, so if Lit ever
  disappears they sign with hot + cold entirely client-side — funds never
  freeze. This fixes the 2-of-2 "lose either share → frozen forever" failure
  mode, and is exactly what the cold-storage share is for.
- DKLs's `initKeyRecovery` / `initLostShareRecovery` cover regenerating a lost
  share. v1 ships 2-of-2; 2-of-3 is a documented variant / follow-up, mechanically
  the same code with different `(participants, threshold)`.

---

## Proposed file layout

```
examples/mpc-signing/
├── README.md
├── package.json
├── package-lock.json
├── hardhat.config.js
├── action/
│   └── mpcSigner.js          # single CID; mode switch over the DKG/sign rounds;
│                             # inlines the base64 wasm; gzip+encrypt session relay
├── client/
│   └── mpcClient.js          # user-side party: drives rounds, persists share_B +
│                             # encrypted_action_session blobs to a local JSON store
├── contracts/
│   └── MpcVault.sol     # minimal vault: exec() requires ecrecover(sig)==signer
├── scripts/
│   ├── _env.js
│   ├── setup.js              # group + usage key + raise response limit; no deploy yet
│   ├── keygen.js             # run DKG with the action; store shares; print address
│   ├── deploy.js             # deploy MpcVault pinning the DKG address
│   └── sign.js               # run signing with the action; submit vault.exec()
└── wasm-demo/                 # ✅ the de-risk scripts (already committed)
    ├── README.md
    ├── smoke.ts
    └── measure.ts
```

Layout matches the existing examples (`README.md`, `action/`, `contracts/`,
`scripts/`, `hardhat.config.js`, `package.json`). New vs the others: `client/`
(first example with a stateful user-side counterpart) and the inlined wasm.

### `action/mpcSigner.js` notes
- Single CID, dispatched by a `mode` param, one mode per protocol round:
  `keygen-r1..r5`, `sign-r1..r5` (exact count per the API above).
- Inlines the 642 KB DKLs wasm as base64 (~856 KB; well under the 16 MB
  code+params budget) and `initSync`s it.
- Each round: decrypt + gunzip the incoming `encrypted_action_session` →
  `fromBytes` → `handleMessages` → `toBytes` → gzip → `Lit.Actions.Encrypt` →
  return alongside the public protocol messages.
- Uses the `ethers` global only for keccak/address helpers; signing math is all
  in the wasm.

### `client/mpcClient.js` notes
- Node CLI (consistent with the other examples' `scripts/*.js`), `fetch` against
  `/core/v1/lit_action`.
- Persists `share_B` (Keyshare bytes), the rolling `encrypted_action_session`,
  the public key, and the derived address to a local JSON file.
- Clean API: `await mpc.keygen()`, `await mpc.sign(digest)`.
- A browser version (the user's tab as the second party) is an explicit v2.

### `contracts/MpcVault.sol`
Minimal; proves the sig verifies like any ECDSA sig:
```solidity
function exec(address to, uint256 value, bytes calldata data, bytes calldata sig) external {
    bytes32 digest = keccak256(abi.encode(address(this), block.chainid, nonce++, to, value, data));
    require(ECDSA.recover(MessageHashUtils.toEthSignedMessageHash(digest), sig) == signer, "bad sig");
    (bool ok,) = to.call{value: value}(data); require(ok, "call failed");
}
```
The contract neither knows nor cares that MPC produced the signature — the
point of doing real threshold ECDSA: full `ecrecover` compatibility.

---

## Milestones

1. ✅ **PR 0 — WASM de-risk (done).** `wasm-demo/` proves wasm loads in Deno from
   inlined bytes, the relay pattern works for DKG + signing, and the output is
   `ecrecover`-compatible. Found + resolved the 100 KB response-size constraint.
2. **PR 1 — action keygen.** `action/mpcSigner.js` DKG modes + `client/mpcClient.js`
   keygen driver + `scripts/keygen.js`. Inlined wasm, gzip+encrypt relay, the
   response-limit raise in `setup.js`. End state: user runs keygen, gets an EVM
   address Lit can't sign for alone.
3. **PR 2 — action signing.** Signing modes + `sign()` driver. Produce a sig,
   verify it recovers to the keygen address off-chain.
4. **PR 3 — `MpcVault.sol` + deploy + on-chain submit.** Wire the sig into
   `vault.exec()`; confirm on-chain.
5. **PR 4 — README + walkthrough.** Same structure as the other example READMEs:
   trust-model diagram, the "why not a Safe" portability answer, the
   response-limit note, and the 2-of-3 cold-storage variant.

## Roadmap (beyond v1)

- **Demo #2 — FROST flagship** (`lit-frost` → WASM): multi-curve threshold
  Schnorr/EdDSA, native on **Bitcoin Taproot** and **Solana**, using Lit's own
  Kudelski-audited crypto. Reuses this demo's proven WASM-in-action platform.
  On EVM, FROST needs a Schnorr verifier contract (the `ecrecover` trick, ~5k
  gas) rather than native `ecrecover` — see notes in the discussion.
- **2-of-3 cold-storage variant** of this same ECDSA example.
- **Presigning** to shrink the online signing blobs (also addresses the
  response-size constraint structurally).
- **Browser client** — the user's tab as the second party (real UX vs CLI).

---

## Library landscape (why DKLs23, and what we ruled out)

We researched Lit's own MPC stack (`lit-rust-crypto`, `hd-keys-curves-wasm`,
`lit-peer`) and the broader org. Findings:

- **`lit-rust-crypto`** — not MPC; a curve-primitive re-export wrapper (re-exports
  `vsss-rs`). **`hd-keys-curves-wasm`** — HD derivation + curve ops +
  *verification* only, and not actually wasm-bindgen-ready despite the name.
- **`lit-peer`** uses first-party MPC crates: **cait-sith** (threshold ECDSA,
  MIT), **`lit-frost`** (threshold Schnorr/EdDSA, ~10 ciphersuites, Kudelski-
  audited, Apache-2.0), **blsful** (threshold BLS). Protocols are clean
  transport-agnostic state machines — repurposable over an HTTP relay.
- **Critical:** the node's *production* threshold-ECDSA ("DamFast", `lit-fast-ecdsa`)
  is **honest-majority**, which is wrong for an adversarial 2-party (Lit + user)
  setup — there is no honest majority in 2-of-2. So we can't reuse the node's
  ECDSA path for demo #1. Lit's `cait-sith` *is* dishonest-majority and could
  back a first-party ECDSA build later, but it's unaudited-as-a-fork, staler, and
  not JS-exposed.
- **No Lit crate is exposed to JS for threshold *signing* today** (the shipped
  `@lit-protocol/wasm`/`ecdsa-sdk`/`bls-sdk` only do client-side share
  *combination*). So there's no first-party shortcut for demo #1.

Net: for a dishonest-majority 2-party **ECDSA** demo that ships now, the
audited third-party **DKLs23** is the right call. For the multi-curve flagship,
Lit's own audited **`lit-frost`** is the right foundation (demo #2). The
encrypt-relay pattern, CID binding, and on-chain verification are identical
across both.

## References

- [`@silencelaboratories/dkls-wasm-ll-web`](https://www.npmjs.com/package/@silencelaboratories/dkls-wasm-ll-web) — DKLs23, t-of-n, `wasm-pack -t web`, Deno-tested. Trail-of-Bits audited (Feb 2024). v1.2.0, 642 KB wasm.
- [DKLs23 overview](https://dkls.info/)
- [`@silencelaboratories/ecdsa-tss`](https://www.npmjs.com/package/@silencelaboratories/ecdsa-tss) — pure-JS Lindell17 2-of-2. Considered, rejected (2-of-2 only).
- [`lit-frost`](https://github.com/LIT-Protocol/lit-frost) / [`cait-sith`](https://github.com/LIT-Protocol/cait-sith) — Lit's first-party threshold crates (demo #2 / future ECDSA).
- Lit runtime is Deno: `lit-actions/Cargo.toml` (Deno v2.2.2). WASM support: `lit-actions/tests/it.rs:785`.
- Limits: `docs/lit-actions/limits.mdx` — 16 MB code+params, **100 KB response** (the constraint above), 64 MB memory, 15 min, 10 sig requests/action.
- Proof scripts: [`examples/mpc-signing/wasm-demo/`](../examples/mpc-signing/wasm-demo/).
