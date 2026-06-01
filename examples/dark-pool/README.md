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
    │  side,px,qty       │ sign order w/         │                        │
    │                    │ trader key            │                        │
    ├───────────────────►│  run action           │                        │
    │                    ├──────────────────────►│ Decrypt(db url)        │
    │                    │                        │ Encrypt(order+sig)     │
    │                    │                        │ INSERT (epoch,pair,ct)─►│
    │  escrow base/quote TO this epoch ─────────────────────────────────────────►  DarkPoolSettlement
    │                                                          (locked until settle)

  Close epoch

  cron / runEpoch.js     matchEpoch (TEE)                              DarkPoolSettlement
    │  run action            │ Decrypt(db url)                              │
    ├───────────────────────►│ SELECT ciphertext[] WHERE epoch ◄── Neon     │
    │                        │ Decrypt each order (in enclave)              │
    │                        │ VERIFY each trader sig; drop forged/replayed │
    │                        │ uniform-price auction → clearingPx, fills    │
    │                        │ sign(epoch,pair,clearingPx,fills) w/ CID key │
    │◄───────────────────────┤ { clearingPx, fills, signature }             │
    │  settleEpoch(epoch, clearingPx, fills, sig) ─────────────────────────►│
    │                                                  ecrecover == matcher ✓
    │                                                  move epoch escrow @ clearingPx
```

The match action can't open a raw Postgres socket — a Lit Action only has HTTP
`fetch` — so the database is **Neon**, queried over its SQL-over-HTTP endpoint.
The connection string is itself an encrypted secret: `setup.js` encrypts it
against the vault PKP and stores only the ciphertext, and the actions decrypt it
inside the enclave at runtime.

Each order is **signed by its trader**. `matchEpoch` verifies that signature
inside the enclave before an order can be matched, so nobody — not even the
operator holding the usage key — can forge an order for an address they don't
control. Escrow is **locked to a specific epoch** until that epoch settles, so a
matched trader can't pull funds out from under a pending settlement.

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
| order side / price / quantity / trader | Neon, as ciphertext | nobody — opaque without the vault PKP |
| order plaintext | TEE memory during the match only | the enclave; never persisted, returned, or logged |
| DB connection string | `.env` (ciphertext) + TEE at runtime | nobody — encrypted against the vault PKP |
| epoch id, pair, `created_at`, `settled` | Neon, plaintext row metadata | the DB operator — this leaks pair routing, per-epoch order *count*, submission *timing*, and settlement status (not order contents). Use one table/db per pair to hide the pair. |
| escrow deposits | on-chain, pre-trade | public — reveals *that* an address escrowed for an epoch, not its order |
| fills (trader, qty, price) | on-chain, post-settlement | public — by design, like real dark-pool trade reporting |

So "the DB holds only ciphertext" is true for *order contents*, but the rows
still carry plaintext routing/timing metadata. The strong claim is narrow and
exact: side, price, quantity, and trader are never readable off the database.

## Files

| Path | Purpose |
|---|---|
| `action/encryptOrder.js` | Decrypts the DB url, seals an order, inserts the ciphertext over HTTP. Returns only `{ ok, id }`. |
| `action/matchEpoch.js` | Decrypts the DB url + the epoch's order batch, runs the uniform-price auction, signs the fills. The pure auction is unit-tested. |
| `contracts/DarkPoolSettlement.sol` | Per-epoch locked escrow + `settleEpoch` that verifies the matcher signature and moves escrow at the clearing price into withdrawable proceeds. |
| `contracts/TestToken.sol` | Minimal mintable ERC-20 standing in for the base and quote assets. |
| `scripts/setup.js` | Computes CIDs, mints the vault PKP, wires the group with the **pinned** CIDs + usage key, encrypts the DB url, applies the schema, derives the matcher address, deploys the contracts. |
| `scripts/submitOrder.js` | Signs the order with the trader key, seals it, and escrows the backing tokens for the epoch. |
| `scripts/runEpoch.js` | Closes an epoch: match, verify the matcher signature, settle on-chain, mark orders settled. |
| `schema.sql` | The `orders` + `epochs` tables. Order contents are ciphertext; rows carry plaintext epoch/pair/timing metadata. |

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

Each order is signed by its trader and bound to the deployed settlement
contract, so `setup` must have deployed the contracts (run it with
`DEPLOYER_PRIVATE_KEY` set). Submit a crossing pair from two trader keys, then
close the epoch:

```bash
# buyer (signs + escrows quote with DEPLOYER_PRIVATE_KEY by default)
npm run submit -- --side buy  --price 100 --qty 5 --epoch 1
# seller (signs + escrows base with its own key)
npm run submit -- --side sell --price 100 --qty 5 --epoch 1 --key 0xSELLER_PRIVATE_KEY

npm run run-epoch -- --epoch 1
```

You'll see how many orders authenticated (and how many were rejected), the
uniform clearing price, the fills, the matcher signature verified locally, and
the on-chain settlement transaction. After it settles, call `withdrawProceeds()`
(base bought / quote received) and `withdrawEscrow(epoch)` (any unspent escrow).

Run the tests (settlement contract + the pure auction + order authentication):

```bash
npx hardhat test
```

## Security model & limitations

This is a teaching example. It demonstrates the confidential-compute mechanic
end to end and closes the obvious holes (order authentication, locked escrow,
replay/conservation), but it deliberately stops short of a production exchange.
Be honest about what you're trusting:

- **You trust the TEE and the matcher.** Order privacy rests on the enclave not
  leaking; settlement integrity rests on the contract trusting whatever the
  matchEpoch CID signs. That's the hardware-privacy trade from the trust section.
  TEEs have known side-channel attacks.
- **Operator liveness is assumed.** Escrow for an epoch is locked until that
  epoch settles. If the operator never calls `run-epoch`, those funds stay
  locked. A production version needs a timeout/cancel path so traders can
  reclaim escrow from an abandoned epoch.
- **The operator chooses epoch membership and timing.** It can't forge or read
  orders, but it decides which submitted orders are in the book when it closes
  an epoch (it could censor or delay specific ones). Order *count* and timing
  also leak via plaintext row metadata (see the privacy table).
- **Single matcher, single enclave.** No redundancy or fault-proof; a buggy or
  unavailable matcher stalls the pool.
- **Test tokens, one pair, no fees.** `TestToken` is freely mintable; there's no
  fee logic, no multi-pair routing, no partial fills across epochs.

If you build on this, treat the above as the to-do list, not as solved.

## Going further

- **Remove the setup-only helpers from the group.** Setup pins four CIDs:
  `encryptOrder`, `matchEpoch`, and two setup helpers (`encryptSecret`,
  address-deriver). Only `matchEpoch` decrypts orders, so leaving the helpers is
  safe, but for the tightest runtime set you can remove the two helper CIDs from
  the group after setup so only the two operational actions remain.
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
| An order can only be placed by the address it names | `matchEpoch` verifies each order's trader EIP-191 signature in the enclave; forged/foreign orders are dropped before matching |
| No duplicate / cross-epoch / cross-pool order replay | nonce + `(chainId, settlement, epoch, pairHash)` bound into the trader's signature; `matchEpoch` dedups `(trader, nonce)` |
| Settlement is signed by the exact match action that was deployed | On-chain: `DarkPoolSettlement` pins the CID-derived `matcher` address |
| Order contents are unreadable off the database | Side/price/quantity/trader are ciphertext; only the pinned `matchEpoch` CID can decrypt them (the group pins specific CIDs — no wildcard) |
| A matched trader can't withdraw before settlement | Escrow is locked to the epoch until `epochSettled[epoch]`; proceeds/leftover only withdrawable after |
| Matched volume conserves (base bought == base sold) | On-chain conservation check + the auction's exact-rationing rule |
| No epoch settles twice | `epochSettled` mapping; the settlement digest also binds `pairHash`/`address(this)`/`chainid` |
| The match code doesn't leak orders or the connection string | **You** — audit `matchEpoch.js` against its IPFS CID before trusting it |
