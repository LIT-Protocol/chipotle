# Keyless Zcash Wallet

**A Zcash transparent (`t1…`) wallet that can only ever be operated by one
exact Lit Action — no private key to hold, no PKP to mint, no program to
deploy. The action derives a secp256k1 address from its own CID-bound identity,
builds every transaction it sends, and signs only capped P2PKH transfers.**

The [`solana-signer`](../solana-signer) example needed a curve bridge because
Solana uses ed25519. Zcash transparent addresses are **secp256k1 / P2PKH** —
the same curve `Lit.Actions.getLitActionPrivateKey()` already returns — so the
identity key *is* the Zcash key, no bridge. What's Zcash-specific, and what this
example teaches, is the **signature hash**: Zcash signs transparent inputs with
a **BLAKE2b-256 digest personalized by the consensus branch ID** (ZIP-243), not
Bitcoin's double-SHA256 and not an EVM keccak hash.

## The idea

`Lit.Actions.getLitActionPrivateKey()` returns a 32-byte secp256k1 private key
derived from the action's IPFS CID. A Zcash transparent address is just the
Bitcoin P2PKH construction with Zcash's version bytes:

```javascript
const priv = hexToBytes((await Lit.Actions.getLitActionPrivateKey()).replace(/^0x/, ""));
const pubkey = secp.getPublicKey(priv, true);            // 33-byte compressed
const h160 = ripemd160(sha256(pubkey));                  // hash160
const address = base58CheckEncode(concat([0x1c, 0xb8], h160));  // t1… (mainnet P2PKH)
```

The key is derived from the CID and never leaves the Lit TEE, so the wallet is
**bound to the code**: change a byte, the CID changes, the key changes, the
address changes. This exact action is the only thing that can ever sign for
that address.

The action does **not** trust the caller to tell it what the transaction is. The
client supplies UTXOs and a recipient + amount; the action **builds every
output itself** — the recipient output (capped) plus a change output that can
only return to its own address — then signs. It will only produce a transaction
when:

- the recipient is a valid mainnet **`t1` P2PKH** address (decoded and
  checksum-verified inside the action),
- the amount to the recipient is **at most `MAX_AMOUNT_ZAT`** (0.01 ZEC),
- the fee is **at most `MAX_FEE_ZAT`** (0.0005 ZEC), and
- **change can only go back to the action's own address** — there is no caller
  path to add an output.

Because the action emits the exact bytes the client broadcasts, a caller can't
redirect the change, exceed the cap, or burn an arbitrary fee. Lying about a
UTXO's value or outpoint can't steal funds either: both are committed in the
ZIP-243 sighash, so a lie just makes the broadcast fail.

```
   client (Blockchair REST)            Lit Action (zcashSigner)               Zcash mainnet
        │                                  │                                      │
        │ action:"address"                 │ derive secp256k1 key from CID        │
        ├─────────────────────────────────►│ hash160 + Base58Check                │
        │◄─────────────────────────────────┤ t1 address                           │
        │  fund the t1 address with ZEC ────────────────────────────────────────►│
        │  GET /dashboards/address (UTXOs) ◄──────────────────────────────────────┤
        │                                  │                                      │
        │ action:"sign" + inputs + recip   │ validate policy; build v4 tx;        │
        ├─────────────────────────────────►│ ZIP-243 BLAKE2b sighash per input;   │
        │◄─────────────────────────────────┤ secp256k1-sign; return raw tx hex    │
        │  POST /push/transaction ──────────────────────────────────────────────►│
```

## ⚠️ This example uses mainnet (real ZEC)

Zcash **testnet** REST infrastructure is effectively dead — the old
`explorer.testnet.z.cash` Insight API is gone, there is no public testnet
Blockbook, and Blockchair has no Zcash testnet. So unlike the Solana example
(devnet, free funds), this one runs on **mainnet with real ZEC** via
[Blockchair's](https://blockchair.com/api) public REST API.

The spend cap is baked into the action's (CID-bound) source at **0.01 ZEC**, so
fund the wallet with only a small amount. The cap, the fee cap, and the network
are all part of the hashed code — changing any of them changes the address.

## Files

| Path | Purpose |
| --- | --- |
| `action/zcashSigner.js` | The Lit Action. Derives the secp256k1 t1 address from the action's identity key, validates the requested spend, builds the v4 transaction, computes the ZIP-243 BLAKE2b sighash for each input, secp256k1-signs, and returns the raw tx hex. Imports `@noble/secp256k1`, `@noble/hashes` (blake2b/sha256/ripemd160/hmac), `@scure/base` (pinned ESM from jsDelivr). |
| `scripts/_lit.js` | Runs the action against `/lit_action` with the scoped usage key and unwraps the response envelope. |
| `scripts/_zcash.js` | Blockchair REST client: list UTXOs, read the chain tip, broadcast a raw tx. |
| `scripts/_env.js` | Minimal `.env` reader / upserter, inlined so the folder is self-contained. |
| `scripts/setup.js` | One-shot: create the group, mint a scoped usage key, derive + record the t1 address, register the action. No contract to deploy. |
| `scripts/address.js` | Print the action's t1 address (re-derived live). |
| `scripts/balance.js` | Show the wallet's confirmed balance and UTXOs. |
| `scripts/transfer.js` | Select UTXOs, have the action build + sign the transfer, broadcast to mainnet. |

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

`BLOCKCHAIR_API_KEY` is optional; set it only if the free tier rate-limits you.

> The scripts use Node's built-in `fetch`, so Node 18+ is required. `npm
> install` has nothing to fetch (the client has no runtime deps) — it just sets
> the folder up.

### 2. Run setup

```bash
npm run setup
```

Six steps: compute the action CID, create a wildcard permission group, mint a
scoped usage key, **derive the t1 address by running the action's `"address"`
branch**, register the action, and add its CID to the group. The address is
written to `.env` as `ZCASH_ADDRESS`.

A freshly-minted usage key's group grant is eventually consistent, so the
first action call (step 4) polls with retries until the grant propagates rather
than aborting on a transient miss — you may see a few `...action not ready yet`
lines before it succeeds.

### 3. Fund the wallet

```bash
npm run address    # print the t1 address
npm run balance    # show balance + UTXOs once funds land
```

Send a small amount of ZEC (the cap is 0.01 ZEC) to the address from any wallet
or exchange, wait for a confirmation, then `npm run balance` should show it.

### 4. Send ZEC

```bash
# Sign + broadcast a 0.001 ZEC transfer to any t1 address:
npm run transfer -- <t1recipient> 0.001
```

The client picks UTXOs, the action builds + signs the transaction, and the
client broadcasts it. Try `npm run transfer -- <recipient> 0.05` to watch the
action **refuse** anything over the 0.01 ZEC cap.

## Why the binding holds

- **Code-bound key.** The secp256k1 key is derived from the action's CID and
  never leaves the Lit TEE. There is no key file to steal; the only way to
  produce a signature for this address is to run this exact code in the network.
- **The signer builds what it signs.** The action constructs every output
  itself — recipient (capped) and change (only ever back to itself). A caller
  can't smuggle in an extra output, redirect the change, or exceed the caps.
- **Tamper-evident policy.** `MAX_AMOUNT_ZAT`, `MAX_FEE_ZAT`, the network
  prefix, and the consensus branch ID are all part of the hashed source, so
  changing any of them changes the CID and the wallet address.
- **Lying about inputs only fails the broadcast.** A UTXO's value is committed
  in its ZIP-243 sighash and its outpoint must reference a real unspent output,
  so a caller that misreports either produces a transaction the network simply
  rejects — it can't redirect funds.

## Production notes

- **Per-user wallets.** Like the [`action-bound-wallet`](../action-bound-wallet)
  example, you can give each user their own Zcash wallet by stamping a value
  (e.g. their id) into the action source — a different CID yields a different
  key and address. Gate spending on the user's own signature recovered inside
  the action.
- **Consensus branch ID is a maintenance constant.** The sighash is
  personalized with the active network upgrade's branch ID (NU6,
  `0xc8e71055`, here). It's the same on mainnet and testnet and only changes at
  a network upgrade — bump `BRANCH_ID_LE` in the action when the next upgrade
  activates. (The CID/address won't change from a branch-ID bump alone only if
  you don't edit the file; editing it *does* change the address, so plan the
  rotation.)
- **v4 transactions.** This builds version-4 (Sapling, ZIP-243) transparent
  transactions, which are simpler to get byte-exact and are still valid on the
  current network. [ZIP-2003](https://zips.z.cash/zip-2003) proposes disallowing
  v4 at a future NU7 (no activation date as of writing); migrating to v5
  (ZIP-244) means swapping the flat sighash preimage for ZIP-244's BLAKE2b hash
  tree.
- **Transparent only.** Shielded (Sapling/Orchard) addresses need zk-SNARK
  proving and are out of scope for a single Lit Action; this example handles
  transparent `t1` P2PKH only.
- **Widen the policy deliberately.** This action signs single-recipient
  transfers with change back to self. Multi-recipient sends, P2SH, or other
  scripts mean extending the builder and policy — and each change mints a new
  CID and a new wallet.
- **Verify before mainnet volume.** Cross-check the sighash against the
  official [zcash-test-vectors](https://github.com/zcash-hackworks/zcash-test-vectors)
  if you adapt this for anything beyond small demo amounts.
