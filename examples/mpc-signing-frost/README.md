# MPC Signing (FROST / Ed25519, Solana)

Threshold **Schnorr** via **FROST** on **Ed25519** — the signature scheme Solana
uses natively. This is the EdDSA sibling of the
[`mpc-signing-ecdsa`](../mpc-signing-ecdsa) example (DKLs23 threshold ECDSA for
EVM); same trust model, different curve and target chain.

> ✅ **Status: working end-to-end on the live Lit network + Solana devnet** (see
> [Verified on the live network](#verified-on-the-live-network)). 2-of-3 FROST
> DKG, plus hot+Lit and hot+cold(recovery) transfers signed and confirmed on
> devnet. The **wasm-bindgen wrapper** over
> [`lit-frost`](https://github.com/LIT-Protocol/lit-frost) (Kudelski-audited
> signing) + [`frost-dkg`](https://github.com/mikelodder7/frost-dkg) (real DKG)
> builds to both targets (`wasm/`, `./build.sh`); building it needed three small
> fixes to the Lit crates, two filed as upstream PRs
> ([frost-dkg#1](https://github.com/mikelodder7/frost-dkg/pull/1),
> [lit-frost#1](https://github.com/LIT-Protocol/lit-frost/pull/1); patches in
> `wasm/*.patch`). **Not production-hardened:** the wasm is served from a personal
> GitHub repo via jsDelivr (move to a Lit-owned npm package) and the patches need
> to land upstream. Not run on mainnet.

**A signing key split between a Lit Action and the user, where Lit literally
cannot produce a signature on its own — and the full private key never exists
anywhere, not even momentarily inside the action.**

Every other example in this folder is a "Lit signs on your behalf" flow: the
action holds a key and can sign whenever its code decides to. That is the wrong
model if you want a key that is **non-custodial** — one Lit provably cannot use
without you personally co-signing each operation.

This example does it with real **threshold FROST** (Flexible Round-Optimized
Schnorr Threshold,
[RFC draft](https://datatracker.ietf.org/doc/draft-irtf-cfrg-frost/), via Lit's
audited [`lit-frost`](https://github.com/LIT-Protocol/lit-frost)), and it
defaults to **2-of-3**:

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
  network — and move your funds.

The output is a standard Ed25519 signature. On Solana it is **indistinguishable
from a normal account signature** and verifies with the chain's native Ed25519
check — no program to deploy, no multisig, nothing on-chain reveals the key is
shared. The FROST group public key (32 bytes) *is* the Solana address.

> Want the minimal version? `npm run keygen -- --basic` does a plain **2-of-2**
> (Lit + your hot share, no recovery share). Everything below works the same —
> you just give up the recovery path.

## Why FROST / Ed25519 and not the ECDSA example?

`mpc-signing-ecdsa` produces secp256k1-ECDSA signatures — one share controls
every ECDSA chain (all EVM chains, Bitcoin legacy/SegWit, Tron, …). But ECDSA
and Schnorr/EdDSA are different protocols with **incompatible shares**: a
secp256k1-ECDSA share cannot sign for Solana.

Solana (and Bitcoin Taproot, Polkadot/Substrate, Zcash, Cardano, …) use Schnorr
or EdDSA signatures. FROST is the threshold protocol for those. `lit-frost`
covers the whole family of FROST ciphersuites (Ed25519, Ed448, Ristretto255,
secp256k1-Schnorr, Taproot, P-256, P-384, Schnorrkel/Substrate, Pallas, Jubjub,
Decaf377) — this example uses **`Ed25519Sha512`** because that is Solana's
native scheme. The same code, with a different `Scheme`, signs for any of them.

So the honest framing across the two examples: **one share per signature-scheme
family, many chains each, no multisig contract, plus a recovery share so you're
never locked out.**

## Trust model

```
   ┌────────────────────────────────────────────┐
   │ User's machine                              │
   │   ├── hot share   (party 0, local)          │   ← never uploaded
   │   ├── cold share  (party 2) → move OFFLINE  │   ← recovery; idle day-to-day
   │   └── encrypted_action_keyshare             │   ← sealed to the group PKP;
   │       + per-round sealed signing nonce      │     useless to anyone but the action
   └─────────────┬───────────────────────────────┘
                 │ each round: { sealed_action_state, the commitments it needs }
                 ▼
   ┌────────────────────────────────────────────┐
   │ Lit Action (party 1, this CID, stateless)   │
   │  decrypt → restore nonce/state              │
   │  signing_round1 / signing_round2(...)       │   ← runs FROST in WASM
   │  reseal nonce → Lit.Actions.Encrypt         │   ← reseals to the group PKP
   │  return { its commitment / sig share }      │
   └────────────────────────────────────────────┘

   day-to-day quorum:  hot (party 0) + Lit (party 1)
   recovery quorum:    hot (party 0) + cold (party 2)  — fully local, no Lit
```

- **The full key never exists**, by construction of FROST DKG. Each party only
  ever holds its own additive/Shamir share; a compromised V8 isolate holding the
  action's share cannot produce a signature without a second party contributing
  its share.
- **No one signs alone.** Every signature needs two of the three shares running
  the protocol together — *hot + Lit* day-to-day, *hot + cold* for recovery.
- **PKP-bound seal.** The action is stateless and the node has no storage, so the
  action's secret state (its signing share, and its per-signature FROST nonce) is
  sealed with `Lit.Actions.Encrypt({ pkpId })` — a ciphertext only actions
  permitted in the group can decrypt. The user stores the blob and relays it back.
- **Nonce-reuse guard — the critical one for FROST.** A FROST signing nonce must
  **never** be reused across two different messages: reuse leaks the signer's
  secret share outright (it is two linear equations in the same secret). The
  action commits the message into the sealed nonce in round 1 and **refuses in
  round 2 to produce a signature share for any other message** — so a malicious
  user relaying the same sealed nonce against a second message cannot extract the
  share. (The ECDSA example has the exact same guard for the same reason.)

## How it works

Both parties run the same FROST library — the action in WASM inside the node,
the user via the Node build locally. The user drives; the action is one
stateless `/core/v1/lit_action` call per round.

- **DKG (`npm run keygen`)** — FROST distributed key generation (3 rounds:
  `dkg_part1` → `dkg_part2` → `dkg_part3`) between the user's party(ies) and the
  action. Produces the group Ed25519 public key and each party's signing share.
  The user keeps the hot (and, in 2-of-3, the cold) share; the action's share is
  returned sealed to its CID. No trusted dealer — the key is never whole.
- **Signing (`npm run sign`)** — FROST signing is **2 rounds**:
  1. **commit** — each signer runs `signing_round1` → a nonce (secret) +
     commitment (public). The action seals its nonce (binding the message hash).
  2. **sign share** — given the message and *all* commitments, each signer runs
     `signing_round2` with its share → a signature share.
  The user then `aggregate`s the two signature shares into a single 64-byte
  Ed25519 signature and submits the Solana transaction.

## Files

| Path | Purpose |
| --- | --- |
| `action/mpcSigner.js` | The Lit Action: the node-side FROST party. Loads the FROST WASM, runs one protocol round per call, seals/relays its share + nonce to its own CID. |
| `client/mpcClient.js` | The user-side FROST party. Drives the rounds, routes commitments/shares, holds the user share(s), aggregates. |
| `client/store.js` | Local JSON store for the user's hot share + the sealed action share (`.mpc-store.json`); the cold recovery share goes to `.mpc-cold-share.json`. Both gitignored. |
| `scripts/setup.js` | Mints the seal PKP, creates + wires the group + scoped usage key, registers the action. No on-chain step (the signer address doesn't exist until keygen). |
| `scripts/keygen.js` | Runs the interactive FROST DKG; writes the shares + Solana address. |
| `scripts/fund.js` | Sends a little SOL to the MPC address (or airdrops on devnet) so it can pay rent/fees. |
| `scripts/sign.js` | Builds a Solana transfer, runs the signing (hot + Lit, or `--recovery` hot + cold), submits it. |

## The FROST WASM build

Authored in [`wasm/`](wasm/) and **builds to both targets** — see
[`wasm/README.md`](wasm/README.md) for the full story (exposed API, the three
build fixes, and the two captured upstream patches). Short version: `cd wasm &&
./build.sh` emits `pkg-web` (for the action) and `pkg-node` (for the client);
the crate compiles to native and `wasm32-unknown-unknown` with 0 errors once the
`frost-dkg` serde patch + `lit-frost` no-blst patch are applied and `multiexp` is
pinned. The right production path is to upstream those two one-liner patches to
the Lit crates and publish the resulting package to npm + jsDelivr.

The ECDSA example works because Silence Laboratories publishes its DKLs23 library
as a wasm-bindgen package on npm + jsDelivr
(`@silencelaboratories/dkls-wasm-ll-web` / `-node`), so the *same* crypto runs in
the Lit Action (Deno, imported from a pinned CDN URL) and on the user's machine
(npm). FROST needs the equivalent:

- **Source:** [`LIT-Protocol/lit-frost`](https://github.com/LIT-Protocol/lit-frost)
  (audited by Kudelski; the FROST core `lit-peer`'s node uses). `lit-frost`
  itself exposes `signing_round1` / `signing_round2` / `aggregate` / `verify`
  but only **trusted-dealer** keygen — non-custodial DKG needs the `frost-dkg`
  crate (`frost-dkg = "0.5.1"` in `lit-peer`). The wrapper must expose **both**.
- **Build:** a `wasm-bindgen` crate wrapping `lit-frost` + `frost-dkg`, compiled
  for `web` (the action) and `nodejs` (the client) — the same two-target pattern
  as [`lit-ecdsa-wasm-combine`](https://github.com/LIT-Protocol/lit-ecdsa-wasm-combine)
  (`build.sh` → npm). Publish to npm and pin a jsDelivr URL for the action.
- **Wrapper API** the `action/` and `client/` code is wired to (see
  `wasm/README.md` for the table): `dkg_round1/2/3`, `sign_round1/2`, `aggregate`,
  `verify`, `verifying_share`. The DKG carries an opaque `state` blob between
  rounds (the action seals it); signing carries the single-use nonce.

**Deploying the action wasm — two real constraints we hit live:**

- The Lit action bundler only resolves **bare npm import specifiers** (it pulls
  them from npm), so a `https://…/gh/…` import is rejected. And the Lit API
  gateway caps request bodies well under the 1.5 MB wasm (a fully-inlined ~2 MB
  action gets `413`), so the wasm **can't** be inlined into the action either.
- The path that works without npm-publish access: **`npm run build:action`
  inlines just the ~25 KB wasm-bindgen glue** into `action/mpcSigner.bundled.js`
  (no `import` for the bundler to choke on, ~33 KB total — well under the gateway
  limit) and the action **`fetch`es the 1.5 MB wasm at runtime** from a CDN (a
  runtime fetch, not a bundler import, so a full URL is fine). The client uses the
  local `pkg-node` by path.

The cleanest production setup is to **publish `lit-frost-wasm` to npm**, then the
action can import it by bare specifier exactly like the ECDSA action imports DKLs
(no glue inlining, no `build:action`). Until then `action/mpcSigner.js` keeps that
npm-import form as the documented target, and `build:action` produces the
glue-inlined deployable. The wasm is currently fetched from a personal repo
(`cdn.jsdelivr.net/gh/clawdbot-glitch003/lit-frost-wasm@v0.0.1`) — **move this to a
Lit-owned npm package or repo.**

## Verified on the live network ✓

Run against the live Lit network + Solana **devnet**, end to end:

- **`setup`** — PKP minted, permission group, scoped usage key, action registered. ✓
- **`keygen` — 2-of-3 FROST DKG** — the action fetched the wasm from the CDN, ran
  `frost-dkg` in its Deno isolate, and sealed its state to the PKP via
  `Lit.Actions.Encrypt` across the two HTTP calls; all parties agreed on the
  group key. **First attempt.** ✓
- **hot + Lit sign + submit** — a real `SystemProgram.transfer` signed by the
  user's hot share co-signing with the Lit Action (2 FROST rounds), confirmed
  on devnet. Lit could not have produced it alone. ✓
- **hot + cold recovery sign + submit** — the same transfer signed entirely
  client-side (no Lit), confirmed on devnet — the 2-of-3 self-custody escape
  hatch, on-chain. ✓
- Both signatures verified as **standard Ed25519** and were accepted by Solana's
  native check — indistinguishable from an ordinary account signature.

Also verified off-network via `pkg-node`: all three quorums ({hot,Lit},
{hot,cold}, {Lit,cold}) produce valid signatures over the same key, and they
verify under `tweetnacl` (rejecting tampering).

> Not yet run on Solana **mainnet** or hardened for production — see
> [Production considerations](#production-considerations) (notably: the FROST
> wasm is currently served from a personal GitHub repo via jsDelivr; move it to a
> Lit-owned npm package, and upstream the two crate patches).

## Walkthrough

```bash
npm install
(cd wasm && ./build.sh)    # build the FROST wasm (see wasm/README.md for its two patches)
npm run build:action       # inline the wasm into action/mpcSigner.bundled.js
cp .env.example .env       # set LIT_API_KEY and FUNDER_SECRET_KEY
npm run setup              # mint PKP, group, scoped usage key, register action
npm run keygen             # 2-of-3 FROST DKG; prints your Solana address
npm run fund               # seed the address with a little devnet SOL
npm run sign -- --to <recipient> --sol 0.01          # hot + Lit sign + send
npm run sign -- --to <recipient> --sol 0.01 --dry    # produce + verify locally, no chain
npm run sign -- --to <recipient> --sol 0.01 --recovery   # hot + cold, no Lit
```

`--recovery` runs the whole signing locally with the hot + cold shares: no Lit
Action, no HTTP, nothing leaves the machine. It signs for the *same* address, so
funds never freeze. (`--dry --recovery` verifies it without touching the chain.)

## Production considerations

- **Inline + pin the WASM.** As in the ECDSA example, for maximum trust inline
  the FROST wasm as base64 in the action so its **CID commits to the exact crypto
  bytes**, rather than fetching from a CDN at runtime.
- **Nonce hygiene.** FROST nonce reuse is catastrophic — it leaks the secret
  share. The action's per-round nonce is single-use and bound to the committed
  message; never relax that guard.
- **Backups.** Hot + Lit is one signing quorum; hot + cold is the other. Keep the
  cold share offline and separate from the hot store.
- **Seal binding.** The action's share is sealed to the group **PKP**, so it
  survives action edits as long as the PKP + group are unchanged. To bind the
  seal to this exact action's bytes, restrict the group's `cid_hashes_permitted`
  to its CID instead of the wildcard `["0"]`.

## References

- [`lit-frost`](https://github.com/LIT-Protocol/lit-frost) — Lit's audited FROST
  library (Kudelski Security audit in its `audit/` dir).
- [FROST paper](https://eprint.iacr.org/2020/852.pdf) /
  [IETF FROST RFC](https://datatracker.ietf.org/doc/draft-irtf-cfrg-frost/).
- [ZCash Foundation FROST](https://frost.zfnd.org/) — the upstream FROST crates.
- [`mpc-signing-ecdsa`](../mpc-signing-ecdsa) — the secp256k1-ECDSA sibling
  (verified on prod), which this mirrors.
