# dark-pool — confidential sealed-bid batch auction example

Status: PLANNED (2026-05-28)
Branch: `glitch003/dark-pool-example`
Lands as: `examples/dark-pool/`
Design doc: `~/.gstack/projects/LIT-Protocol-chipotle/chris-glitch003-dark-pool-example-design-20260528-185043.md`

## One-line pitch

A dark pool where orders are submitted **encrypted**, matched **blind inside the
TEE** at a single uniform clearing price, and settled on-chain — the orders never
exist in plaintext anywhere outside the enclave, including in the database that
stores them.

## Why this is the flagship example

The existing four examples (`compliance-transfer-gate`, `cross-chain-token`,
`multi-source-price-oracle`, `prediction-market-oracle`) are all
**compute + sign**: read something, sign an attestation, a contract verifies it.
None of them use Lit's **encryption** primitive and none hold **confidential
state**. The dark pool is the only example that wires together all three of Lit's
superpowers at once:

1. **Encrypt** — orders are sealed with `Lit.Actions.Encrypt` against a vault PKP.
2. **Confidential compute** — they're decrypted and matched only inside the TEE.
3. **Sign** — the match result is signed with the action's CID-derived key and
   verified on-chain by the settlement contract.

That makes it the clearest single demonstration of "serverless functions that can
hold private keys *and* private data."

## Honest trust framing (load-bearing — the README lives or dies on this)

- The claim is **"TEE-private, async, radically simpler than MPC"** — NOT "more
  private than Renegade."
- Renegade gives *cryptographic* privacy (MPC: no single party ever sees the
  book). We give *hardware* privacy (the TEE sees the book but doesn't leak it).
- The trade: a cryptographic guarantee for a hardware assumption (TEEs have known
  side-channel attacks). In exchange: no MPC ceremony, true async batching,
  confidential state on a $7/mo Postgres, and it actually ships.
- Pre-trade privacy (orders hidden until matched → no front-running of resting
  orders) is the property we deliver. Post-trade, fills settle on-chain and are
  public — exactly like real dark pools report executed trades after the fact.
  Say this plainly in the README.

## Why a sealed-bid batch auction, not a continuous order book

A continuous book matches on time-priority, which reintroduces latency races and
front-running — the disease a dark pool exists to cure — and forces a
stateless-sequencing/locking problem on top of Lit Actions (which are stateless,
one-shot, content-addressed). A sealed-bid batch auction with a single uniform
clearing price:

- removes time-priority → no ordering advantage by construction,
- matches one batch in one atomic enclave run → no per-call sequencing problem,
- is the design CoW Protocol / Penumbra actually use.

Smaller to build *and* the more defensible design.

---

## Architecture

```
  trader              submitOrder.js          encryptOrder            Postgres
  wallet              (orchestrator)           (Lit Action / TEE)     (ciphertext)
    │                      │                         │                    │
    │ order(side,px,qty)   │  encrypt request        │                    │
    ├─────────────────────►│────────────────────────►│ Encrypt(pkpId,     │
    │                      │                         │  JSON(order))      │
    │                      │◄────────────────────────┤  ciphertext        │
    │                      │  INSERT (epoch, pair, ciphertext) ──────────►│
    │ deposit escrow ─────────────────────────────────────────────────────────►  DarkPoolSettlement
    │                      │                         │                    │              (escrow)

  ── epoch closes ──

  cron / runEpoch.js       matchEpoch                                    DarkPoolSettlement
  (orchestrator)           (Lit Action / TEE)                            (on-chain)
    │  SELECT ciphertext[] WHERE epoch=N ◄── Postgres                          │
    │  js_params: { pkpId, ciphertexts[], epoch, pair, contract, chainId }     │
    ├────────────────────────────►│ Decrypt each (pkpId)                       │
    │                             │ uniform-price auction → clearingPx, fills  │
    │                             │ sign digest(epoch, fills, clearingPx) with │
    │                             │ getLitActionPrivateKey()                   │
    │◄────────────────────────────┤ { clearingPx, fills[], signature }         │
    │  settleEpoch(epoch, fills, clearingPx, sig) ─────────────────────────────►│
    │                                                          ecrecover == matcher ✓
    │                                                          move escrowed balances
```

### Key constraint: the action talks to Postgres over HTTP, with an encrypted credential

Lit Actions can only make HTTP `fetch` calls — they cannot open a raw Postgres
TCP socket. So the hosted database **must expose an HTTP SQL API**:

- **Provider: Neon "SQL over HTTP"** (default) — POST raw SQL with the connection
  string in a header. Keeps `schema.sql` and the "it's just Postgres" mental
  model. Alternative: PostgREST/Supabase (plain REST). A TCP-only Postgres
  (Render/RDS/Fly) will NOT work from inside the action.
- The **DB connection string is itself an encrypted secret**: `setup.js` encrypts
  `DATABASE_URL` against the vault PKP and stores only the ciphertext
  (`ENCRYPTED_DATABASE_URL`). The action decrypts it inside the TEE at runtime,
  uses it for the HTTP query, and never returns or logs it (docs' "secure an RPC
  URL with an embedded API key" pattern). This is a **third** showcase of Lit
  encryption, on top of the orders themselves.
- Therefore the **actions own all DB I/O**:
  - `encryptOrder` encrypts the order AND `INSERT`s it (one call).
  - `matchEpoch` `SELECT`s the batch, decrypts, matches, signs.
  - The orchestrator scripts hold no raw DB creds at runtime; they just trigger
    actions and submit the settlement tx.
- The match action is still trivially auditable against its CID: you can read
  exactly what SQL it runs and confirm it never returns plaintext orders or the
  connection string.

### Encryption model (Chipotle, not the old Naga SDK)

- Encryption uses `Lit.Actions.Encrypt({ pkpId, message })` /
  `Lit.Actions.Decrypt({ pkpId, ciphertext })` inside the action. The symmetric
  key is derived from a **vault PKP** inside the TEE.
- Access control = **group membership**: a group binds {vault PKP, permitted
  action CIDs, usage key}. An action can only `Encrypt`/`Decrypt` against the PKP
  if its CID is in the group.
- The "edit-a-byte-and-you're-locked-out" guarantee still holds: change the match
  action source → new CID → it's no longer the permitted CID in the group →
  it can't decrypt, AND its derived signer address changes → the contract stops
  trusting it. One content hash governs both "who may read the orders" and "whose
  settlement the contract trusts."

### The matching algorithm (uniform-price sealed-bid call auction)

Each order: `{ side: "buy"|"sell", limitPrice, quantity, trader }`. Inside
`matchEpoch`:

1. Decrypt all ciphertexts → order list.
2. Find clearing price `p*` that maximizes executable volume:
   - `demand(p)` = Σ qty of buys with `limitPrice >= p`
   - `supply(p)` = Σ qty of sells with `limitPrice <= p`
   - executable at `p` = `min(demand(p), supply(p))`; pick `p*` maximizing it
     (candidate prices = the set of distinct limit prices).
3. Eligible: buys with `limit >= p*`, sells with `limit <= p*`. All fills at `p*`.
4. The long side is rationed **pro-rata** to match the short side's volume
   (deterministic rounding rule, documented; remainder dropped/carried — pick one
   and write it down).
5. Build `fills[] = { trader, side, filledQty, price: p* }`.
6. Sign `keccak256(abi.encode(epoch, pair, clearingPx, fillsHash, contract,
   chainId))` with `getLitActionPrivateKey()`.

Deterministic, self-contained, no external calls. Good for a clean example.

### Settlement + custody

- Traders **pre-deposit escrow** into `DarkPoolSettlement` before the epoch
  closes: sellers escrow the base asset, buyers escrow quote (limit × qty cap).
- `settleEpoch(epoch, fills, clearingPx, signature)`:
  - verify `block`/epoch not already settled (replay protection keyed by epoch),
  - recover signer from the digest, require `== matcher` (pinned at deploy),
  - for each fill, move escrowed balances at `clearingPx`,
  - refund unfilled/rationed escrow,
  - emit `EpochSettled` + per-fill events.
- v1 uses two test ERC-20s (base/quote) deployed by setup. Keep it to one pair.

### Privacy analysis (put this table in the README)

| Data | Where | Visible to whom |
|---|---|---|
| order side / price / qty | Postgres (ciphertext) | nobody — gibberish without the vault PKP |
| order plaintext | TEE memory during match only | the enclave; never persisted |
| DB connection string | `.env` (ciphertext) + TEE at runtime | nobody — encrypted against the vault PKP |
| epoch id, trading pair | Postgres (plaintext routing) | DB operator (hide pair via one DB/table per pair if needed) |
| escrow deposits | on-chain, pre-trade | public — leaks that *someone* may trade, not their order |
| fills (trader, qty, price) | on-chain, post-settlement | public — by design, like real dark pool trade reporting |

---

## Data model (Postgres)

```sql
create table orders (
  id           bigserial primary key,
  epoch        bigint      not null,
  pair         text        not null,         -- e.g. "BASE/QUOTE"
  ciphertext   text        not null,         -- Lit.Actions.Encrypt output
  created_at   timestamptz not null default now(),
  settled      boolean     not null default false
);
create index orders_epoch_pair_idx on orders (epoch, pair) where not settled;

create table epochs (
  epoch        bigint primary key,
  pair         text not null,
  status       text not null default 'open', -- open | matching | settled
  clearing_px  numeric,
  settled_tx   text,
  closed_at    timestamptz
);
```

No order plaintext ever hits these tables. Only ciphertext + routing metadata.

## File layout (mirror the other examples)

```
examples/dark-pool/
  README.md                      sharp writeup + ASCII diagram + privacy table + honest trust framing
  package.json                   compile / setup / submit / run-epoch scripts
  hardhat.config.js              same network config as siblings (baseSepolia default)
  .env.example                   LIT_API_KEY, DEPLOYER_PRIVATE_KEY, DATABASE_URL (HTTP), DB_HTTP_PROVIDER, ...
  schema.sql                     the tables below (applied to the hosted DB at setup)
  action/
    encryptOrder.js              decrypt DB url -> Encrypt(order) -> INSERT ciphertext over HTTP
    matchEpoch.js                decrypt DB url -> SELECT batch -> Decrypt all -> uniform-price match -> sign
  contracts/
    DarkPoolSettlement.sol       escrow + verify matcher sig + settle fills at clearing price
    TestToken.sol                minimal ERC-20 (base & quote) for the demo
  scripts/
    _env.js                      .env load/upsert helper (copy from siblings)
    sql.js                       tiny HTTP-SQL helper SHARED by actions (inlined into action source at setup)
    setup.js                     CIDs, vault PKP, encrypt DB url, group wiring, usage key, deploy contracts, create schema
    submitOrder.js               call encryptOrder action -> deposit escrow tx
    runEpoch.js                  call matchEpoch action -> settleEpoch tx
```

Note: no `docker-compose.yml` / `pg` / `db.js` — the DB is a hosted HTTP-SQL
service and all DB access happens inside the actions over `fetch`.

---

## Phases (check off as you go)

### Phase 0 — De-risk the one risky primitive — ✅ DONE (2026-05-28, GO)
- [x] Write a throwaway action that `Encrypt`s JSON orders against a vault PKP.
- [x] Write a second action that `Decrypt`s the whole batch (passed via `js_params`) and runs the uniform-price match.
- [x] Confirm it runs within action time/memory limits — swept N = 5→200 against the live API, all OK.
- [x] Determine a safe **max batch size** per epoch — see results below.
- [x] GO/NO-GO → **GO.** Batch-decrypt-in-one-run works with large headroom; the rest is well-trodden.

**Spike results** (`.context/dark-pool-spike/spike.js`, live API):
in-enclave decrypt scales linearly at ~2.9ms/order. 200 orders decrypt+match in
one action run in ~0.6s wall; the sweep never hit a ceiling (200 was the cap, not
a limit). Extrapolation says thousands of orders before any action time limit.

| Batch | encrypt | decrypt+match (wall) | in-enclave decrypt |
|---|---|---|---|
| 5 | 56ms | 66ms | 19ms |
| 25 | 101ms | 120ms | 78ms |
| 50 | 194ms | 198ms | 152ms |
| 100 | 331ms | 349ms | 302ms |
| 200 | 689ms | 638ms | 577ms |

Decision: set v1 per-epoch cap at **200 orders** (conservative, well under the real
ceiling) and `log()` if an epoch exceeds it. Revisit only if a demo needs more.
Confirmed mechanics for the real build: `pkpId` = PKP `wallet_address` from
`/create_wallet`; group with `cid_hashes_permitted:["0"]` + `add_pkp_to_group`;
`/lit_action` must run under a scoped usage key (master key can't execute).

### Phase 1 — Scaffold the example — ✅ DONE (2026-05-28)
- [x] Structure/conventions mirrored from `compliance-transfer-gate` (package.json, hardhat.config.js, scripts/_env.js, .gitignore, .env.example tailored for the dark pool).
- [x] Added `schema.sql` (orders + epochs tables, ciphertext-only).
- [x] ~~docker-compose.yml / scripts/db.js / pg~~ REMOVED 2026-05-28: DB is now a hosted HTTP-SQL service (Neon), queried from inside the actions over `fetch` with an encrypted connection string. No local Postgres, no `pg` dependency.
- [x] `npm install` (576 pkgs) + `hardhat compile` → "Compiled 6 Solidity files successfully (evm target: cancun)". Confirmed OZ v5 + solc 0.8.24 resolve.
- [x] `TestToken.sol` landed early (it's the Phase-2 demo ERC-20) to make the compile a real smoke test.
- [x] Verified `.gitignore` excludes node_modules/artifacts/cache/.env; only scaffold + lockfile tracked.

### Phase 2 — Contracts — ✅ DONE (2026-05-28)
- [x] `TestToken.sol` — minimal mintable ERC-20 (used for both base and quote).
- [x] `DarkPoolSettlement.sol`:
  - [x] escrow deposit/withdraw for base and quote (internal-balance model),
  - [x] `matcher` address immutable, set at deploy,
  - [x] `settleEpoch(epoch, clearingPx, fills, sig)` with `ecrecover == matcher`,
  - [x] per-epoch replay guard (`epochSettled`),
  - [x] balance moves at `clearingPx`; over-collateral/unfilled stays as withdrawable balance,
  - [x] conservation invariant (base bought == base sold) as safety net,
  - [x] `pairHash` + `address(this)` + `chainid` bound into the digest (no cross-pool/cross-chain replay),
  - [x] events: `BaseDeposited`/`QuoteDeposited`/`*Withdrawn`, `EpochSettled`, `Filled`.
- [x] Hardhat tests — 6 passing: happy path + withdraw, wrong-signer, replay, under-collateralised, conservation-violation, cross-pool-replay.

**LOCKED — the digest the matchEpoch action MUST reproduce byte-for-byte:**
```
inner  = keccak256(abi.encode(
           uint256 epoch, bytes32 pairHash, uint256 clearingPx,
           bytes32 keccak256(abi.encode(Fill[] fills)),
           address settlement, uint256 chainId))
sign   = personal_sign(inner)            // EIP-191, matches wallet.signMessage(arrayify(inner))
Fill   = tuple(address trader, bool isBuy, uint256 quantity)
pairHash = keccak256(bytes("BASE/QUOTE"))
```
PRICE CONVENTION (locked): `clearingPx` = quote-units per 1 base-unit × 1e18;
`quoteCost = qty * clearingPx / 1e18`. Verified in `test/DarkPoolSettlement.test.js`.

### Phase 3 — encryptOrder action — ✅ CODE DONE (live test pending DB)
- [x] Provider = **Neon SQL-over-HTTP**. Protocol confirmed from the driver source (`src/httpQuery.ts`): `POST https://<host>/sql`, headers `Neon-Connection-String` + `Neon-Array-Mode:true`, body `{query, params}` ($1 placeholders), response `{rows, fields, rowCount}`. Helper inlined in each action (self-contained = auditable) instead of a separate `sql.js`.
- [x] `action/encryptOrder.js`: validate shape → `Decrypt` db url → `Encrypt(order)` → `INSERT (epoch,pair,ciphertext)` over HTTP. Returns only `{ ok, id }`; never the order or the url.
- [x] Gating decision: v1 minimal — the scoped usage key + group membership is the gate (documented in the action header).
- [x] LIVE (2026-05-29): submitted 4 orders via `/core/v1/lit_action` against Neon; rows are ciphertext (332–334 hex chars); a `buy`/`sell`/`limitPrice` cleartext scan of the stored rows returns 0. Privacy claim verified in practice.

### Phase 4 — matchEpoch action (the heart) — ✅ CODE DONE + auction unit-tested (live test pending DB)
- [x] `action/matchEpoch.js`: `Decrypt` db url → `SELECT` open batch for (epoch,pair) over HTTP (cap MAX_BATCH, hard error if exceeded) → `Decrypt` each → `runAuction` → fills.
- [x] Deterministic conservation-safe rationing: floor + largest-remainder (remainder desc, then id asc), so `sum(buy fills) == sum(sell fills)` exactly and settlement never reverts.
- [x] Builds the exact digest `DarkPoolSettlement.settleEpoch` expects; signs with `getLitActionPrivateKey()`. Returns `{ clearingPx, fills, orderIds, signer, signature }`.
- [x] `test/auction.test.js` — 6 passing: simple cross, rounding-dust conservation, no-cross, volume-max + tie-break, fuzz-ish conservation, determinism. (Pure logic extracted from the action source — single source of truth.)
- [x] Digest parity: action uses the identical ethers encoding the contract test proves `settleEpoch` accepts (`tuple(address,bool,uint256)[]`, `toEthSignedMessageHash`).
- [x] LIVE (2026-05-29): ran matchEpoch on epoch 1 (4 orders) — clearing price 100.0, buys 5+3 matched against sells rationed 3.2+4.8 (conserves), signature recovers to the pinned matcher `0x657Bc2…`. Confirms the action's digest == what `settleEpoch` verifies.

### Phase 5 — Orchestration scripts — ✅ CODE DONE; confidential path LIVE (on-chain settle pending deployer key)
- [x] `setup.js`: ran live — computes both CIDs, creates vault PKP, group + usage key, encrypts `DATABASE_URL` → `ENCRYPTED_DATABASE_URL`, applies schema to Neon, derives matcher address, registers actions. Contract deploy is conditional on `DEPLOYER_PRIVATE_KEY` (skips cleanly without it). `scripts/lit.js` shared helper + `scripts/deploy.js` written.
- [x] `submitOrder.js`: ran live — `--side --price --qty --trader/--key --epoch`; scales human price/qty to chain units; encrypt+insert happen in the action; escrow deposit is conditional on deployed contracts.
- [x] `runEpoch.js`: ran live — calls matchEpoch, prints fills, verifies the matcher signature locally (script↔action digest parity), settles on-chain + marks orders settled (via `MARK_SETTLED_CODE` action) when contracts + key are present.
- [x] ON-CHAIN (2026-05-29, Base Sepolia): deployed tokens + settlement (`0x30917C39ba83CeDB56091aE4Eb0e998be70779A7`, matcher pinned `0x657Bc2…`); ran a 2-party cross on epoch 2; settled tx `0x9fed95ea…`. Post-settle: buyer 5 base / 0 quote, seller 0 base / 500 quote; orders + epoch flipped to settled. Balances move at the clearing price exactly as designed.

### Phase 6 — README + docs — ✅ DONE (2026-05-29)
- [x] `examples/dark-pool/README.md`: pitch, honest trust framing, ASCII diagram, "why Lit-shaped", privacy table, file table, run instructions, "what's enforced where", "going further".
- [x] Added the dark-pool row to `examples/README.md`.

(original checklist:)
- [ ] README: pitch, ASCII diagram, "Why this is a Lit-shaped problem", **honest trust framing**, **privacy table**, file table, run instructions, "going further" (continuous book, Postgres-over-HTTP action, lit-triggers for epoch close, hiding the pair).
- [ ] Add a row to `examples/README.md`.
- [ ] (Optional) short docs page under `docs/lit-actions/examples.mdx` cross-link.

### Phase 7 — End-to-end + polish
- [x] Full live run: `npm run setup` → submit crossing orders from 2 wallets → `npm run run-epoch` → on-chain settlement at the expected uniform clearing price (epoch 2 above).
- [x] Confirmed Postgres holds only ciphertext + routing metadata (cleartext scan for buy/sell/limitPrice = 0 rows).
- [ ] Negative checks (nice-to-have demo): edit matchEpoch by a byte → its CID-derived address changes → contract rejects the signature (covered by the `InvalidMatcherSignature` unit test; could also show live). With the wildcard group, decryption is gated by the usage key, not the CID — so frame the live demo around the SIGNING pin.
- [ ] Lint/format to match repo conventions; final read-through of README against the "sophisticated reader" test.

---

## Decisions (locked in office hours 2026-05-28)
1. Sealed-bid batch auction, uniform clearing price — not a continuous book.
2. Approach A: encrypt-to-vault-PKP + ciphertext in Postgres; orchestrator owns DB I/O, action is pure.
3. Honest framing: TEE-private / async / simpler than MPC, not "more private than Renegade."
4. Speed of Postgres reads is a non-issue for the demo (user confirmed).

## Open questions (resolve during build, none are blockers)
- Exact pro-rata rounding/remainder rule at the margin (pick deterministic, document).
- ~~Max batch size per epoch from Phase 0~~ — RESOLVED: cap at 200 (see Phase 0 results).
- Escrow amount for buyers: cap at `limitPrice × qty`; refund the difference vs `clearingPx`. Confirm in contract tests.
- Whether to hide the trading pair too (one table/DB per pair) or accept pair as public routing metadata in v1 (lean: accept it, note it).

## Out of scope for v1 (mention as "going further")
- Continuous order book, partial-fill carry across epochs, cancels/amends.
- Multiple pairs, fee logic, market orders.
- Automated epoch close via `lit-triggers` (v1 uses a manual `runEpoch.js`).
- Hiding post-trade fills (would need a privacy-preserving settlement layer).
