# MPC Signing (ECDSA)

Threshold **ECDSA** (DKLs23). A FROST/Schnorr variant for Bitcoin Taproot and
Solana is a separate example.

**A signing key split between a Lit Action and the user, where Lit literally
cannot produce a signature on its own — and the full private key never exists
anywhere, not even momentarily inside the action.**

Every other example in this folder is a "Lit signs on your behalf" flow: the
action holds a key (its own derived identity, or a PKP) and can sign whenever
its code decides to. That's the right model for attestations and oracles. It is
the *wrong* model if you want a key that is **non-custodial** — one that Lit
provably cannot use without you personally co-signing each operation.

This example does it with real **threshold ECDSA** (the
[DKLs23](https://dkls.info/) protocol, via Silence Laboratories'
Trail-of-Bits-audited WASM library), and it defaults to **2-of-3**:

| Share | Held by | Role |
| --- | --- | --- |
| 1 | **Lit** (sealed to the group PKP) | co-signer |
| 2 | **You — hot** (this machine) | day-to-day signing |
| 3 | **You — cold** (offline) | recovery |

A distributed key generation (DKG) splits the key into these three shares; any
two can sign. That buys two things at once:

- **Lit is a required co-signer, not a custodian.** Day-to-day you sign with
  *hot + Lit*. Neither share alone can sign, and the key is never assembled —
  so a compromised action, or a malicious node, can't extract anything signable.
- **You can never be locked out.** Because you hold 2 of the 3 shares, if Lit
  ever disappears you sign with *hot + cold* entirely client-side — no Lit, no
  network — and move your funds. That kills the classic 2-of-2 failure mode where
  losing either party freezes the key forever.

The output is a standard secp256k1 ECDSA signature. On-chain it is
indistinguishable from a normal EOA signature and verifies with plain
`ecrecover`.

> Want the minimal version? `npm run keygen -- --basic` does a plain **2-of-2**
> (Lit + your hot share, no recovery share). Everything below works the same —
> you just give up the recovery path.

## Why not just a multisig contract?

A fair question — on EVM, a 2-of-3 multisig contract gets you "any two of three
must approve." The reasons to do this with MPC instead:

- **It's a normal address.** No multisig contract to deploy, cheaper, and
  private — the threshold (2-of-3) nature is invisible on-chain.
- **Portability — scoped honestly.** One threshold key is *not* one universal
  key across all chains (ECDSA and Schnorr/EdDSA are different protocols with
  incompatible shares). But **one secp256k1-ECDSA share controls every
  ECDSA-secp256k1 chain** — all EVM chains, plus Bitcoin legacy/SegWit, Tron,
  etc. — with HD-derived addresses and no per-chain contract anywhere. Solana
  (Ed25519) and Bitcoin Taproot (Schnorr) would each use a separate FROST share
  under the same Lit + user model (a future example).

So the honest pitch is "one share per signature-scheme family, many chains
each, no multisig contract, plus a recovery share so you're never locked out" —
strictly more than a per-chain Safe gives you.

## Trust model

```
   ┌────────────────────────────────────────────┐
   │ User's machine                              │
   │   ├── hot share   (party 0, local)          │   ← never uploaded
   │   ├── cold share  (party 2) → move OFFLINE  │   ← recovery; idle day-to-day
   │   └── encrypted_action_keyshare             │   ← sealed to the group PKP;
   │       + per-round sealed session            │     useless to anyone but the action
   └─────────────┬───────────────────────────────┘
                 │ each round: { sealed_action_state, the messages it needs }
                 ▼
   ┌────────────────────────────────────────────┐
   │ Lit Action (party 1, this CID, stateless)   │
   │  decrypt → session.fromBytes                │
   │  handleMessages(...)                        │   ← runs DKLs23 in WASM
   │  session.toBytes → gzip →                   │
   │     Lit.Actions.Encrypt                     │   ← reseals to the group PKP
   │  return { its messages, sealed_state }      │
   └────────────────────────────────────────────┘

   day-to-day quorum:  hot (party 0) + Lit (party 1)
   recovery quorum:    hot (party 0) + cold (party 2)  — fully local, no Lit
```

- **The full key never exists**, by construction of DKLs23. Any single share —
  or a compromised V8 isolate holding the action's share — cannot produce a
  signature without a second party interactively contributing its rounds.
- **No one signs alone.** Lit can't sign alone, and no single user share can
  either. Every signature needs two of the three shares running the protocol
  together — *hot + Lit* day-to-day, *hot + cold* for recovery.
- **PKP-bound seal.** The action is stateless and the node has no storage, so the
  action's own secret state (its keyshare and per-round session) is sealed with
  `Lit.Actions.Encrypt({ pkpId })`. Decryption requires that **PKP**, which only
  actions permitted in the group can use — so the seal is gated by the group's
  PKP + CID permissions, *not* by the calling action's exact bytes. (With the
  wildcard `cid_hashes_permitted: ["0"]` this example uses, any action your usage
  key can run could decrypt; to bind the seal to this exact action, restrict the
  group's permitted CIDs to its CID. That's the tighter production setup.) The
  user stores these blobs and relays them back each round.
- **Relay integrity.** Each sealed blob carries `kind` + `round` + `sessionId`
  tags the action checks, so the user can't splice rounds or mix state across
  sessions. Critically, **the message to sign is committed in sign round 1 and
  bound into the sealed presignature**: round 4 refuses to finalize for any other
  hash. Without that, a malicious user could replay one presignature against two
  digests — reusing the ECDSA nonce, which leaks the key — and the stateless
  action could not otherwise detect it. DKLs's own per-round commitments catch
  fabricated protocol messages.

## ⚠️ Prerequisite: a raised response-payload limit

The action returns its sealed session to the user **in the response** each round
(it has nowhere else to put it). A few rounds — notably signing rounds 2–3,
which carry the OT precomputation — exceed the **default 100 KB response limit**
(`docs/lit-actions/limits.mdx`); even gzipped they're ~140 KB, ~190 KB after the
base64+encryption the seal requires.

There is no API to raise this — **contact Lit (support@litprotocol.com / Discord)
to raise your account's response-payload limit to ~256 KB** before running the
sign step. Keygen and most rounds fit under the default; the heavy signing
rounds do not. (A future presigning variant moves that precompute out of the
online path and removes the requirement.)

## How it works

Both parties run the same DKLs23 library — the action in WASM inside the node,
the user via the Node build locally. The user drives; the action is one stateless
`/core/v1/lit_action` call per round.

- **DKG (`npm run keygen`)** — 5 rounds: `createFirstMessage` + 4 `handleMessages`
  (one carrying the chain-code commitments) → `keyshare()`. Produces the shared
  public key and its EVM address. The user keeps the hot (and, in 2-of-3, the
  cold) share; the action's share is returned sealed to its CID.
- **Signing (`npm run sign`)** — 4 rounds: `createFirstMessage` + 3
  `handleMessages` → `lastMessage(digest)`; the user does the final `combine()`
  locally to assemble `[R, S]`, normalizes to low-s, recovers `v`, and submits.

## Files

| Path | Purpose |
| --- | --- |
| `action/mpcSigner.js` | The Lit Action: the node-side MPC party. Loads DKLs23 WASM, runs one protocol round per call, seals/relays its session to its own CID. |
| `client/mpcClient.js` | The user-side MPC party. Drives the rounds, routes messages, holds the user share(s). |
| `client/store.js` | Local JSON store for the user's hot keyshare + the sealed action keyshare (`.mpc-store.json`); the cold recovery share goes to `.mpc-cold-share.json`. Both gitignored. |
| `contracts/MpcVault.sol` | Minimal vault whose `exec()` requires an `ecrecover`-verified signature from the MPC address. Proves the sig is bog-standard ECDSA. |
| `scripts/setup.js` | Mints the seal PKP, creates + wires the group + scoped usage key, registers the action. No deploy (the signer address doesn't exist until keygen). |
| `scripts/keygen.js` | Runs the interactive DKG; writes the shares + address. |
| `scripts/deploy.js` | Deploys the vault, pinning the DKG address as signer. |
| `scripts/sign.js` | Builds the digest, runs the signing (hot + Lit, or `--recovery` hot + cold), submits `exec()`. |
| `wasm-demo/` | Standalone Deno scripts that show DKLs23 running in the action runtime and surviving the per-round relay — a minimal, offline WASM demo. |

## Walkthrough

### 1. Install + configure

```bash
npm install
cp .env.example .env
```

Set `LIT_API_KEY` (account-level master key), and `DEPLOYER_PRIVATE_KEY` /
`EXECUTOR_PRIVATE_KEY` (an EOA with gas on Base Sepolia).

### 2. Setup

```bash
npm run setup
```

Mints the seal PKP, creates the permission group + scoped usage key, registers
the action. Prints the PKP, action CID, and group ID.

### 3. Generate the key (interactive DKG)

```bash
npm run keygen
```

Runs the 5-round **2-of-3** DKG between your machine and the action. Prints the
**EVM address** Lit cannot sign for without you, writes your hot share + the
sealed action share to `.mpc-store.json`, and writes the **cold recovery share**
to `.mpc-cold-share.json`.

> ⚠️ **Move `.mpc-cold-share.json` offline** (cold storage / a different device)
> and remove it from this machine. It is not used for normal signing — only for
> recovery if Lit ever becomes unavailable.

### 4. Deploy a vault to that address

```bash
npm run deploy:baseSepolia
```

Deploys `MpcVault` with the DKG address as its signer, then fund the vault
with a little Base Sepolia ETH.

### 5. Sign + execute (hot + Lit)

```bash
npm run sign -- --to 0xRecipient --value 0.001
```

Runs the 4-round signing between your **hot** share and the Lit Action, assembles
an `ecrecover`-valid signature, and submits `vault.exec(...)`. The action saw
only its own share and protocol messages — never your share, never the full key.
(Add `--dry` to verify the signature locally without touching the chain.)

### 6. Recovery: sign without Lit

This is the payoff of the 2-of-3 default. Normal signing is hot + Lit and the
cold share stays offline — but because you hold 2 of the 3 shares, if Lit ever
disappears you can still sign with **hot + cold, entirely on your own machine**:

```bash
# restore .mpc-cold-share.json to this machine first
npm run sign -- --to 0xRecipient --value 0.001 --recovery
```

`--recovery` runs the whole signing locally: no Lit Action, no HTTP, nothing
leaves the machine. It signs against the *same* address, so the vault you already
deployed accepts it — funds never freeze. (`--dry --recovery` verifies it without
touching the chain.)

All three quorums ({hot,Lit}, {hot,cold}, {Lit,cold}) produce signatures
verifiable by the same address. The two the scripts expose — {hot,Lit} and
{hot,cold} — are verified on prod; {Lit,cold} is confirmed in the offline harness.

> **✅ Prod status — 2-of-3 works end-to-end on the live network.**
> The multi-party DKG used to fail intermittently mid-protocol — `handleMessages`
> threw **"Missing message"** (round 2) or **"Invalid commitment hash"** (round 4)
> from inside the wasm. The root cause was **node-side**: the `lit_actions` worker
> cached `js_params` across executions, so a multi-peer round could read another
> request's messages / commitment array. That is exactly why **2-of-2 never hit it**
> (single peer per round) — the trigger was specifically the multi-peer (≥2 incoming
> messages / commitment-array) rounds that only t-of-n with n>2 exercises. The
> node-side fix is now deployed: 2-of-3 keygen completes on the **first attempt**
> (verified 7/7 on prod), and both signing quorums produce `ecrecover`-valid
> signatures. `keygen` keeps its whole-DKG retry as belt-and-suspenders for ordinary
> transient network errors. The offline harnesses in `wasm-demo/` are a minimal
> reproduction of the working path.

## The minimal 2-of-2 variant

If you don't want a recovery share, `--basic` does a plain 2-of-2 (Lit + your hot
share):

```bash
npm run keygen -- --basic                    # 2-of-2 DKG; no cold share
npm run sign -- --to 0xRecipient --value 0.001
```

It's simpler, but it's exactly the failure mode the 2-of-3 default exists to fix:
lose either share and the key is gone forever, and there's no way to sign if Lit
becomes unavailable. Use it only if you're handling backups some other way.

## Production considerations

- **Inline + pin the WASM.** This example imports the DKLs glue from a pinned
  jsDelivr URL and fetches the wasm at runtime. For maximum trust, inline the
  wasm as base64 in the action so its **CID commits to the exact crypto bytes**
  (jsDelivr is immutable at a pinned version, but inlining removes the
  dependency entirely).
- **Backups.** Hot + Lit is one signing quorum; hot + cold is the other. Keep the
  cold share (`.mpc-cold-share.json`) backed up **offline and separate** from the
  hot store — it's what lets you recover if Lit disappears or you lose the hot
  machine. (With `--basic` 2-of-2 there is no cold share, so back up the hot store
  some other way — lose it and the key is gone.)
- **Presigning.** DKLs23 supports a presign phase that moves the heavy OT
  precompute out of the online signing path — shrinking the relayed blobs (and
  removing the raised-response-limit requirement) and making online signing
  close to non-interactive.
- **Seal binding.** The action's keyshare is sealed to the group **PKP**, so it
  survives action edits as long as the PKP + group are unchanged (verified). If
  you want the seal bound to a specific action's bytes, restrict the group's
  `cid_hashes_permitted` to that CID instead of the wildcard `["0"]`.

## Verified on production

Run against the live Lit network and Base Sepolia:

- `setup` — PKP, group, scoped usage key, action registration ✓
- **2-of-3** `keygen` (the default) — the multi-party 5-round DKG, exercising the
  action's jsDelivr import + runtime wasm fetch + `initSync`,
  `Lit.Actions.Encrypt`/`Decrypt`, `CompressionStream` gzip, the per-round
  encrypted-session relay, and the (raised) response limit. **Reliable on the live
  network** after the node-side `js_params`-caching fix: **7/7 first-attempt on
  prod**, plus offline 30/30 real client / 64/64 concurrent ✓
- **2-of-3** `sign` (hot + Lit) — the 4-round signing submitted a real on-chain
  `MpcVault.exec` on Base Sepolia, verified via `ecrecover`; the vault paid out
  and bumped its nonce ✓
- **2-of-3** recovery `sign` (hot + cold) — fully local, no Lit; produces an
  `ecrecover`-valid signature against the same address ✓
- **2-of-2** (`--basic`) — `keygen` and `sign` verified the same way, including a
  real on-chain `MpcVault.exec` (single peer per round; reliable throughout) ✓
- **Nonce-reuse guard** — adversarially tested on prod: replaying a built
  presignature into round 4 with a *different* `messageHash` is refused inside the
  action ("nonce-reuse guard"), while the committed hash signs normally ✓

Requires the response-payload limit raised on your account (see the prerequisite
above) — production has been raised to 16 MB. The Deno scripts in
[`wasm-demo/`](./wasm-demo/) are a standalone, offline demonstration of the WASM +
relay mechanics.

## References

- [DKLs23](https://dkls.info/) — the threshold-ECDSA protocol.
- [`@silencelaboratories/dkls-wasm-ll-web`](https://www.npmjs.com/package/@silencelaboratories/dkls-wasm-ll-web) / [`-node`](https://www.npmjs.com/package/@silencelaboratories/dkls-wasm-ll-node) — the WASM library (audited by Trail of Bits, Feb 2024).
- `docs/lit-actions/limits.mdx` — action limits, incl. the 100 KB response cap.
- `docs/lit-actions/imports.mdx` — how the action imports the library from jsDelivr.
