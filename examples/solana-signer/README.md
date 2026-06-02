# Keyless Solana Wallet

**A Solana wallet that can only ever be operated by one exact Lit Action — no
private key to hold, no PKP to mint, no program to deploy. The action derives
an ed25519 keypair from its own CID-bound identity, inspects every transaction
it's asked to sign, and signs only capped `SystemProgram` transfers.**

Most Lit Action examples sign for EVM chains, where the action's identity key
(`Lit.Actions.getLitActionPrivateKey()`) is already a secp256k1 EVM key. Solana
uses **ed25519**, so this example shows the small bridge: an ed25519 Solana
keypair derived from the same action identity, and ed25519 signing inside the
action.

## The idea

`Lit.Actions.getLitActionPrivateKey()` returns a 32-byte secp256k1 private key
derived from the action's IPFS CID. A Solana keypair is just an ed25519 keypair
built from a 32-byte **seed** — which is exactly what Solana's
`Keypair.fromSeed(seed)` does. So we reuse those 32 bytes as the ed25519 seed:

```javascript
const seed = hexToBytes((await Lit.Actions.getLitActionPrivateKey()).replace(/^0x/, ""));
const publicKey = ed.getPublicKey(seed);   // @noble/ed25519
const address = base58.encode(publicKey);  // the Solana address
```

The seed is derived from the CID and never leaves the Lit TEE, so the wallet is
**bound to the code**: change a byte, the CID changes, the seed changes, the
Solana address changes. This exact action is the only thing that can ever sign
for that address.

The action does **not** blindly sign whatever bytes it's handed. It parses the
serialized transaction message and signs only when:

- there's exactly **one required signature** and it's the action's own address
  (the fee payer),
- there's exactly **one instruction**, and it's a `SystemProgram` **transfer**,
- the transfer **debits the fee payer** (the action's wallet), and
- the amount is **at most `MAX_LAMPORTS`** (0.5 SOL, baked into the code — and
  therefore into the CID and the address).

The canonical message bytes are built client-side by `@solana/web3.js`; the
parse inside the action is read-only validation, so a parser quirk can only
ever *reject* — it can never sign something other than the exact bytes the
client broadcasts.

```
   client (@solana/web3.js)           Lit Action (solanaSigner)              Solana devnet
        │                                  │                                      │
        │ action:"address"                 │ derive ed25519 key from CID          │
        ├─────────────────────────────────►│                                      │
        │◄─────────────────────────────────┤ address (base58)                     │
        │  airdrop devnet SOL ──────────────────────────────────────────────────►│
        │                                  │                                      │
        │ build transfer tx, feePayer=addr │                                      │
        │ serializeMessage()               │                                      │
        │ action:"sign" + message + recip  │ parse msg; enforce policy;           │
        ├─────────────────────────────────►│ ed25519-sign the exact bytes         │
        │◄─────────────────────────────────┤ signature (base64)                   │
        │  addSignature + sendRawTransaction ─────────────────────────────────────►│
```

## Files

| Path | Purpose |
| --- | --- |
| `action/solanaSigner.js` | The Lit Action. Derives the ed25519 Solana keypair from the action's identity key, parses the legacy transaction message, enforces the transfer policy, and signs. Imports `@noble/ed25519`, `@noble/hashes`, `@scure/base` (pinned ESM from jsDelivr). |
| `scripts/_lit.js` | Runs the action against `/lit_action` with the scoped usage key and unwraps the response envelope. |
| `scripts/_env.js` | Minimal `.env` reader / upserter, inlined so the folder is self-contained. |
| `scripts/setup.js` | One-shot: create the group, mint a scoped usage key, derive + record the Solana address, register the action. No contract to deploy. |
| `scripts/address.js` | Print the action's Solana address (re-derived live). |
| `scripts/airdrop.js` | Request a devnet SOL airdrop to the wallet. |
| `scripts/transfer.js` | Build a transfer, have the action inspect + sign it, broadcast to devnet. |

## Walkthrough

### 1. Install + configure

```bash
cp .env.example .env
npm install
```

Set in `.env`:
- `LIT_API_KEY` — your **account-level (master) API key** from the
  [Chipotle dashboard](https://dashboard.chipotle.litprotocol.com), *not* a
  scoped usage key (setup calls `/add_group`, which rejects scoped keys).

`SOLANA_RPC_URL` defaults to the public devnet endpoint; override it if you hit
rate limits.

### 2. Run setup

```bash
npm run setup
```

Six steps: compute the action CID, create a wildcard permission group, mint a
scoped usage key, **derive the Solana address by running the action's
`"address"` branch**, register the action, and add its CID to the group. The
address is written to `.env` as `SOLANA_ADDRESS`.

A freshly-minted usage key's group grant is eventually consistent, so the
first action call (step 4) polls with retries until the grant propagates
rather than aborting on a transient miss — you may see a few
`...action not ready yet` lines before it succeeds.

### 3. Fund the wallet

```bash
npm run airdrop            # 1 SOL of devnet funds
npm run address            # show the address (e.g. to use the web faucet)
```

Devnet faucet airdrops are rate-limited and sometimes flaky. If `airdrop`
fails, paste the address into <https://faucet.solana.com> or run
`solana airdrop 1 <address> --url devnet`.

### 4. Send SOL

```bash
# Sign + broadcast a 0.01 SOL transfer to any devnet address:
npm run transfer -- <recipientBase58> 0.01
```

The client builds the transfer, the action inspects and signs it, and the
client broadcasts it. Try `npm run transfer -- <recipient> 1` to watch the
action **refuse** anything over the 0.5 SOL cap.

## Why the binding holds

- **Code-bound key.** The ed25519 seed is derived from the action's CID and
  never leaves the Lit TEE. There is no key file to steal; the only way to
  produce a signature for this address is to run this exact code in the network.
- **The signer inspects what it signs.** The action parses the message and
  enforces single-transfer / fee-payer-is-self / amount-cap policy. A caller
  can't smuggle in a second instruction, redirect the debit, or exceed the cap.
- **Tamper-evident policy.** `MAX_LAMPORTS` and the policy logic are part of the
  hashed source, so changing them changes the CID and the wallet address. The
  cap is bound to the address exactly like the key is.
- **Local signature verification.** `tx.addSignature` verifies the 64-byte
  signature against the fee payer before broadcast, so a mismatch fails on the
  client rather than on-chain.

## Production notes

- **Per-user wallets.** Like the [`action-bound-wallet`](../action-bound-wallet)
  example, you can give each user their own Solana wallet by stamping a value
  (e.g. their id) into the action source — a different CID yields a different
  seed and address. Gate spending on the user's own signature recovered inside
  the action.
- **Recent-blockhash freshness is a liveness input, not a safety one.** The
  client fetches the blockhash; a stale one just makes the broadcast fail
  (Solana rejects expired blockhashes), it can't redirect funds — recipient and
  amount are signature-bound and policy-checked in the TEE.
- **Widen the policy deliberately.** This action only signs `SystemProgram`
  transfers. To support SPL-token transfers or other programs, extend the parser
  and policy — and remember each change mints a new CID and a new wallet.
- **Devnet only as written.** Point `SOLANA_RPC_URL` at mainnet only once you've
  reviewed the policy for your real spending limits.
