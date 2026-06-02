# `mpc-signing` — design notes

A signing flow where **Lit literally cannot produce a signature on its own.** A
DKLs23 threshold-ECDSA DKG splits the key into shares held by the Lit Action and
the user; every signature is an interactive MPC protocol between them, and the
full private key never exists anywhere — not even momentarily inside the action.
The output is a standard secp256k1 ECDSA signature any EVM contract verifies with
plain `ecrecover`.

The example defaults to **2-of-3** — Lit + the user's hot share + a cold recovery
share. Because the user holds 2 of the 3 shares, they can always recover (sign
hot + cold, entirely client-side, no Lit) if Lit ever disappears, which fixes the
2-of-2 "lose either share → frozen forever" failure mode. `keygen --basic` does a
plain 2-of-2 (Lit + hot, no recovery).

This fills a gap in `examples/`: every other example is a "Lit signs on your
behalf" flow where the action can sign whenever its code decides to. This is the
first where the user is a **required co-signer**.

**Owner:** chris@litprotocol.com · **Location:** `examples/mpc-signing/`

## Design decisions

1. **Real MPC, not Shamir.** A "reconstruct a Shamir/additive split inside the
   action" version was considered and dropped: the whole point is that a
   compromised action can't extract the key, and Shamir reconstructs the full key
   in the V8 isolate at sign time. This does real threshold ECDSA where the key
   never exists anywhere.

2. **Library: DKLs23 via `@silencelaboratories/dkls-wasm-ll-web` (WASM)**, not the
   pure-JS Lindell17 (`@silencelaboratories/ecdsa-tss`). Two reasons:
   - DKLs23 is general **t-of-n**, so the same code does **2-of-3 with a
     cold-storage recovery share**. Lindell17 is hard-wired 2-of-2 — a dead end.
   - Paying the **WASM-in-a-Lit-Action** cost here builds the platform the FROST
     flagship (roadmap) needs anyway, which can only ship as WASM (`lit-frost`).

   DKLs23 is Trail-of-Bits audited (Feb 2024); 642 KB wasm + 41 KB JS glue.

3. **EVM / ECDSA first.** EVM is where Lit's audience and all existing examples
   are, and DKLs23 output is a standard ECDSA sig → `ecrecover` works → it drops
   into any contract or holds real ETH at a normal address.

4. **Interactive is fine.** Each signature is a multi-round interactive protocol;
   that's acceptable here. Presigning (which would make online signing nearly
   non-interactive and shrink the relayed blobs) stays a documented optimization.

5. **Why not a Gnosis Safe 2-of-3?** The honest answer is **portability, scoped
   correctly** — don't overclaim. DKLs23 (ECDSA) and FROST (Schnorr/EdDSA) produce
   cryptographically incompatible shares, so there is no single share that signs
   every chain. What's true:
   - **One DKLs23 secp256k1-ECDSA share controls every ECDSA-secp256k1 chain** with
     no per-chain contract: all EVM chains, plus Bitcoin legacy/SegWit, Tron, etc.,
     and many HD-derived addresses via DKLs's `chain_path`.
   - **Solana (Ed25519) and Bitcoin Taproot (Schnorr) need separate FROST shares**
     — same Lit + user model, different key.

   So the accurate pitch is "one share per signature-scheme family, many chains
   each, plus a recovery share" — not "one universal share." Always-true claims: no
   per-chain multisig contract; on EVM the key is a normal address (cheaper,
   private); and the same architecture/UX extends to every chain.

## DKLs23 API & round structure

Classes (all with `toBytes`/`fromBytes`): `KeygenSession`, `SignSession`,
`Keyshare`, `Message`.

- **DKG** (`new KeygenSession(participants, threshold, party_id)` — `(3, 2, …)`
  for the default 2-of-3): `createFirstMessage()` → `handleMessages()` ×4 (one
  carries the chain-code commitments from `calculateChainCodeCommitment()`) →
  `keyshare()`. ≈ 5 message exchanges → 5 action round-trips.
- **Signing** (`new SignSession(keyshare, chainPath)`): `createFirstMessage()` →
  `handleMessages()` ×3 → `lastMessage(msgHash)` → `combine(msgs)` → `[R, S]`. The
  client derives `v` by trial recovery against the known public key (standard).

HD derivation via `chainPath`; `initKeyRotation` / `initKeyRecovery` /
`initLostShareRecovery` are built-in (relevant to recovery and future variants).

## Trust model

```
   ┌────────────────────────────────────────────┐
   │ User's machine                              │
   │   ├── hot share   (party 0, local)          │
   │   ├── cold share  (party 2) → OFFLINE       │  recovery; idle day-to-day
   │   └── encrypted_action_session/keyshare     │  sealed to the group PKP
   └─────────────┬───────────────────────────────┘
                 │ each round: { sealed_action_state, the messages it needs }
                 ▼
   ┌────────────────────────────────────────────┐
   │ Lit Action (party 1, this CID, stateless)   │
   │  decrypt → fromBytes                         │
   │  check sessionId + round (+ messageHash)     │
   │  handleMessages(...)                         │
   │  toBytes → gzip → Lit.Actions.Encrypt        │
   │  return { action msgs, sealed_action_state } │
   └────────────────────────────────────────────┘

   day-to-day quorum:  hot (party 0) + Lit (party 1)
   recovery quorum:    hot (party 0) + cold (party 2)  — fully local, no Lit
```

- **The full key never exists**, by construction of DKLs23. Any single share — or
  a compromised V8 isolate holding the action's share — cannot sign without a
  second party interactively contributing its rounds.
- **PKP-bound seal.** The action is stateless and the node has no storage, so the
  action's own session/keyshare is sealed with `Lit.Actions.Encrypt({ pkpId })`.
  Decryption is gated by the **group PKP + CID permissions**, *not* by the calling
  action's exact bytes — so the seal survives action edits. With the wildcard
  `cid_hashes_permitted: ["0"]` the example uses, any action the usage key can run
  could decrypt; bind the seal to one action's bytes by restricting the group's
  permitted CIDs (the tighter production setup).
- **Relay integrity.** Each sealed blob carries `kind` + `round` + `sessionId`,
  checked on the way back in, so the user can't splice rounds or mix sessions.
  Critically, the **message to sign is committed in sign round 1 and bound into the
  sealed presignature** — round 4 refuses to finalize for any other hash. Without
  that, a malicious user could replay one presignature against two digests,
  reusing the ECDSA nonce and leaking the key, and the stateless action couldn't
  detect it. DKLs's own per-round commitments catch fabricated protocol messages.
- **Output** is a standard secp256k1 ECDSA signature — on-chain it's
  indistinguishable from a normal EOA signature.

Scope to be honest about: the hot share, the sealed action share, and the usage
key all live on the same machine, so the 2-of-3 split protects against
Lit-acting-alone and against Lit-disappearing (recovery) — not against local
compromise of that machine. Policy enforcement on what gets signed is out of
scope here (see `lit-solver-vault` for that pattern).

## 2-of-3 with cold-storage recovery (the default)

`KeygenSession(3, 2, party_id)`:

| Share | Held by | Role |
| --- | --- | --- |
| 1 | Lit (sealed to the group PKP) | co-signer |
| 2 | User — hot (CLI/browser) | day-to-day |
| 3 | User — cold storage | recovery |

Normal signing is Lit + hot; the cold share stays offline. Lit alone can't sign
and hot alone can't sign. Because the user holds 2 of 3, **if Lit ever disappears
they sign with hot + cold entirely client-side** — funds never freeze.
`keygen --basic` produces a plain 2-of-2 (no cold share) for users handling
backups another way.

## The one real constraint: response payload size

The action is stateless, so each round it returns its (encrypted) session to the
user in the **response**, and the default response cap is **100 KB**
(`docs/lit-actions/limits.mdx`). Measured blob sizes (raw / gzipped):

| Phase | raw | gzip | vs 100 KB |
| --- | --- | --- | --- |
| DKG, all rounds | ≤122 KB | ≤79 KB | ✅ fits gzipped |
| Long-lived keyshare (stored between sessions) | 121 KB | 78 KB | ✅ fits gzipped |
| Heavy signing rounds (keyshare + OT/MtA precompute) | 214 KB | 138 KB | ❌ over, even gzipped |

Resolution: every relayed blob is gzipped before `Lit.Actions.Encrypt` (the
runtime has `CompressionStream`), and the example needs its account's response
limit raised (~256 KB; prod is raised to 16 MB). Presigning would remove the
heavy online rounds structurally.

## File layout

```
examples/mpc-signing/
├── README.md
├── package.json / package-lock.json
├── hardhat.config.js
├── action/mpcSigner.js     # single CID; one protocol round per call;
│                           # imports DKLs glue from pinned jsDelivr + fetches the
│                           # wasm at runtime; gzip+encrypt session relay
├── client/
│   ├── mpcClient.js        # user-side party: drives rounds, routes messages,
│   │                       # holds the hot (and cold) share
│   └── store.js            # local JSON store (hot store + cold share file)
├── contracts/MpcVault.sol  # minimal vault: exec() requires ecrecover(sig)==signer
├── scripts/
│   ├── _env.js
│   ├── setup.js            # PKP + group + scoped usage key + action registration
│   ├── keygen.js           # interactive DKG (2-of-3 default; --basic = 2-of-2)
│   ├── deploy.js           # deploy MpcVault pinning the DKG address
│   └── sign.js             # signing (hot+Lit, or --recovery hot+cold) + exec
└── wasm-demo/              # standalone Deno demo that DKLs23 runs in the runtime
    ├── README.md
    ├── smoke.ts            # full DKG+sign round-trip, serialize/restore each round
    └── measure.ts          # relayed blob sizes vs the response limit
```

New vs the other examples: `client/` (first example with a stateful user-side
counterpart). For production, inline + pin the wasm in the action so its CID
commits to the exact crypto bytes (the example fetches from pinned jsDelivr).

`MpcVault.sol` is deliberately minimal — it neither knows nor cares that MPC
produced the signature, which is the point of doing real threshold ECDSA:

```solidity
function exec(address to, uint256 value, bytes calldata data, bytes calldata sig) external {
    bytes32 digest = keccak256(abi.encode(address(this), block.chainid, nonce++, to, value, data));
    require(ECDSA.recover(MessageHashUtils.toEthSignedMessageHash(digest), sig) == signer, "bad sig");
    (bool ok,) = to.call{value: value}(data); require(ok, "call failed");
}
```

## Library landscape (why DKLs23, and what was ruled out)

- **`lit-rust-crypto`** — not MPC; a curve-primitive re-export wrapper.
  **`hd-keys-curves-wasm`** — HD derivation + curve ops + verification only, and
  not actually wasm-bindgen-ready despite the name.
- **`lit-peer`** uses first-party MPC crates: **cait-sith** (threshold ECDSA, MIT),
  **`lit-frost`** (threshold Schnorr/EdDSA, ~10 ciphersuites, Kudelski-audited,
  Apache-2.0), **blsful** (threshold BLS) — clean transport-agnostic state
  machines, repurposable over an HTTP relay.
- **Critical:** the node's production threshold-ECDSA ("DamFast", `lit-fast-ecdsa`)
  is **honest-majority**, which is wrong for an adversarial 2-party (Lit + user)
  setup — there is no honest majority in 2-of-2. Lit's `cait-sith` *is*
  dishonest-majority and could back a first-party ECDSA build later, but it's
  unaudited-as-a-fork, staler, and not JS-exposed.
- **No Lit crate is exposed to JS for threshold signing today** (the shipped
  `@lit-protocol/wasm`/`ecdsa-sdk`/`bls-sdk` only do client-side share
  *combination*).

Net: for a dishonest-majority 2-party **ECDSA** demo that ships now, the audited
third-party **DKLs23** is the right call. For the multi-curve flagship, Lit's own
audited **`lit-frost`** is the right foundation. The encrypt-relay pattern, PKP
binding, and on-chain verification are identical across both.

## Roadmap (beyond this example)

- **FROST flagship** (`lit-frost` → WASM): multi-curve threshold Schnorr/EdDSA,
  native on **Bitcoin Taproot** and **Solana**, reusing this example's proven
  WASM-in-action platform. On EVM, FROST needs a Schnorr verifier contract rather
  than native `ecrecover`.
- **Presigning** to shrink the online signing blobs (also addresses the
  response-size constraint structurally).
- **Browser client** — the user's tab as the second party (real UX vs CLI).

## References

- [`@silencelaboratories/dkls-wasm-ll-web`](https://www.npmjs.com/package/@silencelaboratories/dkls-wasm-ll-web) — DKLs23, t-of-n, `wasm-pack -t web`, Deno-tested. Trail-of-Bits audited (Feb 2024). v1.2.0, 642 KB wasm.
- [DKLs23 overview](https://dkls.info/)
- [`@silencelaboratories/ecdsa-tss`](https://www.npmjs.com/package/@silencelaboratories/ecdsa-tss) — pure-JS Lindell17 2-of-2. Considered, rejected (2-of-2 only).
- [`lit-frost`](https://github.com/LIT-Protocol/lit-frost) / [`cait-sith`](https://github.com/LIT-Protocol/cait-sith) — Lit's first-party threshold crates (future work).
- Limits: `docs/lit-actions/limits.mdx` — 16 MB code+params, 100 KB response (the constraint above), 64 MB memory, 15 min, 10 sig requests/action.
- WASM in actions: `docs/lit-actions/wasm.mdx`. Standalone demo: [`examples/mpc-signing/wasm-demo/`](../examples/mpc-signing/wasm-demo/).
