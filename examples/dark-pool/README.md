# Dark Pool

**A confidential exchange: orders are submitted encrypted, matched blind inside
the TEE at a single uniform clearing price, and settled on-chain — the order's
side, price, and quantity never exist in plaintext anywhere outside the
enclave, including in the database that stores them.**

Every other example in this folder is *compute + sign*: read something, sign an
attestation, a contract verifies it. This one is the first to use Lit's
**encryption** primitive and to hold **confidential state**, which is what a
dark pool actually needs. It wires together all three of Lit's capabilities at
once:

1. **Encrypt** — each order (and the database connection string itself) is
   sealed with `Lit.Actions.Encrypt` against a vault PKP.
2. **Confidential compute** — orders are decrypted and matched only inside the
   TEE, in one atomic run.
3. **Sign** — the match result is signed with the match action's CID-derived key
   and verified on-chain by the settlement contract.

## Honest trust model (read this first)

This is **not** "more private than an MPC dark pool like Renegade." It's a
different trade:

- An MPC dark pool gives *cryptographic* privacy — no single party ever sees the
  book.
- This gives *hardware* privacy — the TEE sees the book but doesn't leak it. You
  are trusting the enclave (and TEEs have known side-channel attacks).

What you get for that trade: no MPC ceremony, true async batching, confidential
state on a $0 hobby-tier Postgres, and an exchange you can stand up in an
afternoon. The privacy property delivered is **pre-trade**: orders are hidden
until they match, so resting orders can't be front-run. **Post-trade, fills
settle on-chain and are public** — exactly like a real dark pool reports
executed trades after the fact.

## Why a sealed-bid batch auction (not a continuous order book)

A continuous order book matches on time-priority, which reintroduces latency
races and front-running — the exact thing a dark pool exists to remove — and
would force a sequencing/locking scheme on top of stateless Lit Actions. A
**sealed-bid batch auction with a single uniform clearing price**:

- removes time-priority, so there is no ordering advantage by construction,
- matches the whole batch in one atomic enclave run, so there is no per-call
  sequencing problem,
- is the design CoW Protocol and Penumbra actually use.

Smaller to build, and the more defensible design.

## How it works

```
  Submit (per order)

  trader            submitOrder.js         encryptOrder (TEE)        Neon (ciphertext)
    │  side,px,qty       │                       │                        │
    ├───────────────────►│  run action           │                        │
    │                    ├──────────────────────►│ Decrypt(db url)        │
    │                    │                        │ Encrypt(order)         │
    │                    │                        │ INSERT (epoch,pair,ct)─►│
    │  (optional) escrow base/quote ───────────────────────────────────────────►  DarkPoolSettlement

  Close epoch

  cron / runEpoch.js     matchEpoch (TEE)                              DarkPoolSettlement
    │  run action            │ Decrypt(db url)                              │
    ├───────────────────────►│ SELECT ciphertext[] WHERE epoch ◄── Neon     │
    │                        │ Decrypt each order (in enclave)              │
    │                        │ uniform-price auction → clearingPx, fills    │
    │                        │ sign(epoch,pair,clearingPx,fills) w/ CID key │
    │◄───────────────────────┤ { clearingPx, fills, signature }             │
    │  settleEpoch(epoch, clearingPx, fills, sig) ─────────────────────────►│
    │                                                  ecrecover == matcher ✓
    │                                                  move escrow @ clearingPx
```

The match action can't open a raw Postgres socket — a Lit Action only has HTTP
`fetch` — so the database is **Neon**, queried over its SQL-over-HTTP endpoint.
The connection string is itself an encrypted secret: `setup.js` encrypts it
against the vault PKP and stores only the ciphertext, and the actions decrypt it
inside the enclave at runtime. The database operator only ever sees ciphertext.

### Why this is a Lit-shaped problem

You can't build this with a normal serverless function, because a normal
function that decrypts the order book is a function whose operator can read the
order book. Here the decrypt + match happens inside the TEE, and the settlement
is signed by a key **derived from the match action's IPFS CID**. The deployed
`DarkPoolSettlement` pins that address as its `matcher`. Edit the match action by
one byte and its CID changes, its derived address changes, and the contract
stops trusting its settlements. The matching logic is therefore as auditable as
its content hash.

## Privacy: what's visible to whom

| Data | Where it lives | Who can see it |
|---|---|---|
| order side / price / quantity | Neon, as ciphertext | nobody — opaque without the vault PKP |
| order plaintext | TEE memory during the match only | the enclave; never persisted, returned, or logged |
| DB connection string | `.env` (ciphertext) + TEE at runtime | nobody — encrypted against the vault PKP |
| epoch id, trading pair | Neon, plaintext routing metadata | the DB operator (use one table per pair to hide it) |
| escrow deposits | on-chain, pre-trade | public — reveals *that* someone may trade, not their order |
| fills (trader, qty, price) | on-chain, post-settlement | public — by design, like real dark-pool trade reporting |

## Files

| Path | Purpose |
|---|---|
| `action/encryptOrder.js` | Decrypts the DB url, seals an order, inserts the ciphertext over HTTP. Returns only `{ ok, id }`. |
| `action/matchEpoch.js` | Decrypts the DB url + the epoch's order batch, runs the uniform-price auction, signs the fills. The pure auction is unit-tested. |
| `contracts/DarkPoolSettlement.sol` | Escrow + `settleEpoch` that verifies the matcher signature and moves balances at the clearing price. |
| `contracts/TestToken.sol` | Minimal mintable ERC-20 standing in for the base and quote assets. |
| `scripts/setup.js` | Computes CIDs, mints the vault PKP, wires the group + usage key, encrypts the DB url, applies the schema, derives the matcher address, deploys the contracts. |
| `scripts/submitOrder.js` | Seals one order (and optionally escrows the backing tokens). |
| `scripts/runEpoch.js` | Closes an epoch: match, verify the signature, settle on-chain, mark settled. |
| `schema.sql` | The `orders` + `epochs` tables. Ciphertext only. |

## Run it

Requires a **Neon** database (its SQL-over-HTTP endpoint is what the action
calls; a plain TCP-only Postgres will not work from inside a Lit Action).

```bash
cp .env.example .env
# Fill in:
#   LIT_API_KEY           account-level Lit API key
#   DATABASE_URL          Neon connection string (postgresql://...neon.tech/...)
#   DEPLOYER_PRIVATE_KEY  Base Sepolia EOA with a little test ETH
npm install
npm run setup            # wires Lit, encrypts the DB url, applies schema, deploys contracts
```

Submit a crossing pair from two wallets, then close the epoch:

```bash
# buyer (uses DEPLOYER_PRIVATE_KEY by default; escrows quote)
npm run submit -- --side buy  --price 100 --qty 5 --epoch 1
# seller (its own key; escrows base)
npm run submit -- --side sell --price 100 --qty 5 --epoch 1 --key 0xSELLER_PRIVATE_KEY

npm run run-epoch -- --epoch 1
```

You'll see the uniform clearing price, the fills, the matcher signature verified
locally, and the on-chain settlement transaction. After it settles, the buyer's
internal `baseBalance` and the seller's `quoteBalance` are withdrawable.

To watch the confidential pipeline without deploying contracts, omit
`DEPLOYER_PRIVATE_KEY`: `setup` skips the deploy, `submit` seals orders without
escrow (pass `--trader 0x...`), and `run-epoch` matches + signs without settling.

Run the tests (contract + the pure auction logic):

```bash
npx hardhat test
```

## Going further

- **Pin the action CIDs in the group.** This example uses a wildcard action
  allowlist (any action the scoped usage key can run may use the vault PKP),
  matching the other examples here. The signing pin is enforced on-chain
  regardless, but to also stop the *operator* from swapping in a different
  decrypt action, replace the wildcard with the two real CIDs in `setup.js`.
- **Automate epoch close** with `lit-triggers` (a scheduled trigger that calls
  `run-epoch`) instead of running it by hand.
- **Continuous order book / partial fills across epochs.** Possible, but it
  brings back the sequencing problem this design avoids — read the trust section
  again before reaching for it.
- **Hide the trading pair** by using one table (or one database) per pair so even
  the routing metadata is uniform.
- **Hide post-trade fills** would require a privacy-preserving settlement layer;
  out of scope here.

## What's enforced where

| Guarantee | Enforced by |
|---|---|
| Settlement is signed by the exact match action that was deployed | On-chain: `DarkPoolSettlement` pins the CID-derived `matcher` address |
| Orders are unreadable to the DB operator and external observers | They are ciphertext; only an action runnable with the operator's scoped usage key can decrypt |
| Matched volume conserves (base bought == base sold) | On-chain conservation check + the auction's exact-rationing rule |
| No epoch settles twice; no cross-pool/cross-chain replay | `epochSettled` mapping + `pairHash`/`address(this)`/`chainid` bound into the signed digest |
| The match code doesn't leak orders or the connection string | **You** — audit `matchEpoch.js` against its IPFS CID before trusting it |
