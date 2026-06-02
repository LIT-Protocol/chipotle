# Private Stablecoin on Lit — Design Plan

Status: exploratory
Author: Chris (with Claude)
Date: 2026-05-27 (updated 2026-05-28)

## Decisions locked in (read this first)

These were debated and settled; the rest of the doc reflects them.

1. **Infrastructure, not issuer.** Lit Labs publishes the contract + Lit
   Action templates and sells them to issuers. Lit Labs never holds
   reserves, never signs mints, never runs the user-facing frontend. This
   is the Uniswap-Labs-vs-the-protocol posture, and it's the single most
   important legal protection (see "Legal posture").
2. **Storage is on-chain, obfuscated via a commitment/nullifier note
   model.** No off-chain ledger database. The chain holds commitments,
   nullifiers, and encrypted note blobs; the Lit Action is the prover that
   replaces a ZK circuit. (See "State model" — this replaces the earlier
   two-place off-chain design.)
3. **OFAC everywhere, KYC only at the dollar edges.** OFAC screening is
   baked into every action, non-optional (it's cheap, automatic, and
   strict-liability). KYC runs only on `mint`/`redeem` — the on/off ramp —
   not on private transfers. Built on the existing
   `examples/compliance-transfer-gate`. Necessary but not sufficient
   legally — it fixes the sanctions prong, not the money-transmitter prong;
   the structural posture in #1 fixes that. (See "Compliance model.")
4. **Sell to issuers via a live testnet demo first.** Phase 0 is the demo;
   it's the thing that closes a regulated buyer.

## Legal posture (why we operate nothing)

OFAC screening alone does not make this safe. The Tornado Cash prosecution
turned on two things: (a) willful blindness to specific sanctioned use
(fixed by non-bypassable OFAC checks), and (b) operating an unlicensed
money-transmitting business and taking protocol fees (NOT fixed by OFAC
checks). The defense that protects Uniswap Labs is structural: Labs doesn't
operate the protocol, holds no admin key, takes no protocol fee.

Our version: Lit Labs publishes immutable contracts + Lit Action templates,
issuers deploy their own instances with their own PKP and their own reserve,
Lit Labs' revenue is upfront licensing / SaaS — not a per-transaction
protocol fee. Engage a fintech attorney (one who's read the Storm verdict)
*before* Phase 1 ships, not before GA. ~$50–100k, and it tells us which of
these structural choices are load-bearing. **Not legal advice — this is
pattern-matching off Tornado / Van Loon / FinCEN guidance and needs a real
lawyer to confirm.**

## Compliance model — identity at the edges, privacy in the middle

OFAC and KYC are different obligations with different costs, and conflating
them is what makes KYC "feel heavy." Separate them:

- **OFAC = a list check.** A yes/no `eth_call`, zero user friction,
  strict-liability, already built. Runs on *every* action, always on.
  Skipping it is the willful-blindness fact pattern that sank Tornado.
- **KYC = identity verification.** Real friction, real PII. Legally required
  only where the regulated activity is — the **fiat boundary**. So:

| Action | OFAC | KYC |
|---|---|---|
| `mint` (USDC → privUSD) | ✓ | ✓ — entering the system |
| `redeem` (privUSD → USDC) | ✓ | ✓ — cashing out |
| `shieldedTransfer` (priv → priv) | ✓ | ✗ — already verified at entry |

This is exactly "you KYC with the service once, then your transfers are
private." Same as real money: you KYC when you open the bank account, your
Venmo payments don't each re-verify you. **Privacy lives inside the
perimeter; identity is checked at the door.**

Two things keep it light:

1. **We never hold KYC data.** It's an *attestation*, not a database. The
   provider (Persona/Sumsub) verifies the user and signs a JWT; the `mint`
   action checks the signature. No PII stored on our side — kills most of
   the breach/compliance surface. The issuer partner owns even that
   relationship.
2. **The KYC threshold is a knob the issuer sets**, because the action is
   just JavaScript: KYC-on-first-mint-only, KYC-above-$X, or tiered limits
   (unverified capped at $1k lifetime, verified unlimited). FinCEN's own
   framework is threshold-based. We sell this as *configurable compliance* —
   a feature, not a tax.

The reference deployment KYCs at the edges. An issuer who wants to run
fully permissionless ("the USDC was already KYC'd by Circle, we just wrap
it") can configure that — but it's their legal entity taking that risk
knowingly, not our default. (That argument is seductive and roughly the
Tornado argument; don't ship it as the default.)

## Why this exists

There are no real private stablecoins. Zcash/Aztec-style shielded pools are
private but uncompliant (no built-in selective disclosure, no KYC hook,
issuers won't touch them). Tornado-style mixers are dead for the same reason.
USDC, USDT, PYUSD, etc. are fully transparent — every payroll, every B2B
invoice, every payment to a contractor is on a public ledger forever.

The gap in the market is: **a stablecoin where balances and transfers are
hidden from the public chain, but the issuer can prove reserves and a
regulator can decrypt specific transactions with a warrant.** That is the
exact shape Lit is good at: programmable signing + access-control
encryption + Lit Actions as a trusted bookkeeper running in the Lit TEE.

We have a structural advantage over both pure-ZK projects (no circuits to
write, compliance hooks are just JavaScript) and over centralized "private"
ledgers (no single trusted party — threshold of Lit nodes + TEE attestation).

## Goal

Ship a working private USD-denominated stablecoin ("**privUSD**" working
name) backed 1:1 by USDC held in a PKP-controlled vault, where:

- Balances are encrypted; only the holder (and authorized parties) can read
  them.
- Transfers reveal nothing on-chain except a state-root update.
- Mint/redeem is gated by KYC attestation + OFAC check inside a Lit Action.
- A regulator with a valid warrant can request selective decryption of a
  specific transaction or account, mediated by a multi-sig of designated
  decryption authorities.
- Reserves are publicly verifiable (the vault PKP address holds N USDC, the
  encrypted ledger's total supply equals N).

## Non-goals

- DEX integration, lending, yield. Just payments and transfers in v1.
- Multi-chain. v1 is one EVM chain (Base, probably — cheapest + Coinbase
  ecosystem trust for compliance story).
- Native mobile app. v1 is web + an SDK that wallets can integrate.
- Decentralized governance / DAO. The issuer is a single legal entity (us,
  or a partner — see "customers" section).

## How it works

### State model — commitment/nullifier notes, fully on-chain

No off-chain ledger database. "Obfuscated on-chain" means the proven
shielded-pool shape (Zcash / Railgun / Aztec all use it), with the Lit
Action playing the role the ZK circuit plays elsewhere.

A balance is not a number in a mapping — it's a set of **notes** the owner
controls. The `PrivUSDPool` contract on Base stores only:

- **Commitments** — `hash(ownerPubkey, amount, salt)`. A note worth some
  amount that reveals nothing. Stored in a `mapping(bytes32 => bool)` the
  action reads directly to confirm a note exists. No Merkle tree: because the
  action (not a ZK circuit) is the prover, it legitimately knows which note
  it's checking, so an O(1) mapping lookup is the right primitive — there's no
  index to hide.
- **Nullifiers** — when a note is spent, a one-way `hash(noteSecret)` tag is
  published so the same note can't be spent twice. Unlinkable to its
  commitment.
- **Encrypted note blobs (in event logs)** — the recipient needs
  `amount + salt` to spend their note later, so the sender posts it
  encrypted *to the recipient* via Lit access control, as an event. This is
  what makes the entire ledger **reconstructable from the chain alone** — no
  database to trust, censorship-resistant, no "you run a secret ledger"
  criticism.
- **Public `totalSupply()`** — incremented on mint, decremented on redeem,
  for the reserve proof.

**The Lit Action replaces the ZK circuit.** Instead of a proof asserting "I
own input notes whose sum ≥ output notes, without revealing which," the Lit
Action runs in the TEE: it decrypts the relevant notes, checks the
arithmetic, checks OFAC/KYC, and threshold-signs the state update the
contract will accept. We trade ~6 months of circuit engineering for ~500
lines of JavaScript. That swap *is* the pitch to an issuer: Aztec-grade
privacy with no circuit team.

Access control on each encrypted note blob:
```
accessControlConditions: [
  { recipient can decrypt their own incoming note },
  { OR: an authorized privUSD Lit Action (CID-pinned) can decrypt },
  { OR: regulator multi-sig presents a valid warrant for this note }
]
```

### The four core Lit Actions

Everything user-facing routes through one of these. Each runs in the Lit TEE
and produces a CID-derived signature the contract
verifies with `ecrecover` — the exact trust model already proven in
`examples/compliance-transfer-gate` (signer address is derived from the
action's IPFS CID; edit a byte → new address → old contract rejects it).

1. **`mint`** — caller deposits USDC to the issuer vault, presents a KYC
   attestation. Lit Action verifies the attestation signature, runs the
   inline OFAC sanctions check (the `compliance-transfer-gate` logic), then
   creates a new output note commitment + encrypted blob and signs the
   `totalSupply += amount` state update.

2. **`shieldedTransfer`** — caller proves ownership of input notes, names
   output note commitments (recipient + change), amount, and supplies the
   encrypted blobs. Lit Action decrypts inputs in the TEE, checks
   `sum(inputs) == sum(outputs)`, runs the OFAC check on the recipient,
   marks input nullifiers spent, and signs the tree update. **The chain
   sees new commitments + nullifiers + encrypted blobs — no amount, no
   parties.**

3. **`redeem`** — caller proves ownership of notes summing to ≥ N, requests
   N privUSD → USDC. Lit Action verifies, nullifies the inputs (mints a
   change note for the remainder), and signs a tx releasing N USDC from the
   issuer vault to the caller's wallet, plus `totalSupply -= N`.

4. **`disclose`** — a regulator multi-sig presents a signed warrant naming a
   note commitment or nullifier. Lit Action verifies the multi-sig, decrypts
   the target note, re-encrypts the result to the regulator's pubkey, and
   returns it. The event `(warrantHash, discloserId, timestamp)` is logged
   on-chain for accountability. **This is the demo's money shot — no other
   private stablecoin can show selective, accountable disclosure.**

### Reserve proof

Public, continuous, no audit firm needed:

- `PrivUSDPool.totalSupply()` is incremented/decremented inside `mint` and
  `redeem`, signed by the same PKP that owns the USDC vault.
- Anyone can call `usdc.balanceOf(vaultPKP)` and confirm `>= totalSupply()`.
- Any divergence is a stop-the-world bug; we run a watcher that pages
  oncall and halts the contract.

### What the chain reveals vs. hides

| Public on chain | Hidden |
|---|---|
| Total supply | Individual balances (note amounts) |
| Commitments + nullifiers | Who paid whom |
| Mint/redeem amounts (issuer flow) | Transfer amounts |
| Vault USDC reserve | Note ↔ wallet mapping |
| Disclosure events (count + warrant hash) | Disclosed contents |

The leak surface is only the Lit nodes — there is no operator database,
because the encrypted notes live in on-chain event logs and are decryptable
only via Lit access control. The Lit-node leak is mitigated by TEE +
threshold (no single node sees plaintext). This is strictly better than the
earlier off-chain-ledger design and removes the "issuer runs a secret
ledger" criticism entirely.

## What we need to build

### Phase 0 — The sales demo (BUILT — `examples/private-stablecoin/`)

This is the thing we put in front of an issuer. It is the highest-leverage
work in the whole plan. Built as a new example beside the one it extends,
reusing `compliance-transfer-gate`'s CID-pinned-oracle trust model wholesale.
**Done and tested** — `npm test` exercises the full on-chain path; the live
`npm run demo` needs Lit creds + Base Sepolia gas.

- [x] **`contracts/PrivUSD.sol`** — commitments + nullifiers + encrypted note
      blobs in events, `totalSupply` for reserve proof, `mint` /
      `shieldedTransfer` / `redeem`, all gated by `ecrecover` against the
      pinned ledger-action signer. `reserveBacked()` view. (`MockUSDC.sol`
      for the testnet reserve.)
- [x] **`action/ledger.js`** — the prover. `op = mint | transfer | redeem`:
      reads chain state to validate input notes, checks `sum(in)==sum(out)`,
      inline Chainalysis OFAC on every recipient, KYC attestation on mint,
      encrypts output notes to the ledger PKP, signs with
      `getLitActionPrivateKey()`. (One dispatch action → one pinned signer,
      simpler than three.)
- [x] **`action/disclose.js`** — verifies a 3-of-5 multi-sig warrant and
      decrypts the named note. The demo's money shot.
- [x] **`scripts/demo.js`** — the 2-minute scripted run: KYC + mint to Alice →
      reserve proof → Alice privately pays Bob (prints the tx's on-chain
      footprint: nullifiers + commitments, no amount/parties) → 3-of-5
      warrant decrypts only Bob's note while the rest stay dark (and a 2-of-5
      warrant is rejected).
- [x] **`scripts/setup.js` / `deploy.js`** — idempotent: mint ledger PKP,
      compute both action CIDs, wire the group + scoped usage key + PKP
      authorization, deploy to Base Sepolia.
- [x] **`test/privusd.test.js`** — 6 passing tests: mint/transfer/redeem
      state + reserve conservation, forged-signature / replay / double-spend
      rejection. Validates ethers↔Solidity `abi.encode` equivalence and the
      sign/recover path without the Lit network.

Testnet only. Issuers want to see it work, not custody real money yet.

**Still to do for a live investor demo:** run `npm run setup` + `npm run demo`
against real Lit creds on Base Sepolia and capture a recording; measure the
`transfer` p99 latency (the load-bearing assumption for the payroll pitch);
add a hosted reserve-dashboard page.

### Phase 1 — Internal alpha (4–6 weeks)

- [ ] **`PrivUSDPool` contract**, production version of the Phase 0
      contract on Base. Commitment mapping, nullifier set,
      `totalSupply`, issuer-vault USDC custody, oracle-rotation setter
      behind a multisig.
- [ ] **Lit Actions** hardened for the four flows above, living in
      `lit-assets/lit-actions/src/privUSD/`. Multi-source OFAC consensus
      (the `multi-source-price-oracle` pattern) instead of single-provider.
- [ ] **KYC attestation flow** — integrate one provider (Persona or
      Sumsub). They sign a JWT-shaped attestation the `mint` action
      verifies. (No KYC database on our side — the attestation is the
      artifact.)
- [ ] **Web SDK** — `@lit-protocol/privusd-sdk`, wraps the action calls so
      a partner can `await privusd.transfer({ to, amount })` from a dapp,
      handling note selection + blob encryption client-side. ~500 lines of
      TS over `@lit-protocol/lit-node-client`.
- [ ] **Reserve dashboard** — public page showing
      `usdc.balanceOf(vault) ≥ totalSupply()` updated each block. (Doubles
      as sales collateral.)
- [ ] **Note-scanning** — client/SDK reconstructs balance by scanning
      on-chain encrypted blobs it can decrypt. No indexer to trust, but add
      an optional caching indexer for UX.

### Phase 2 — Pilot (4 weeks after phase 1)

- [ ] Wallet integrations: at least one (Coinbase Wallet or MetaMask
      Snap) where privUSD shows up as a normal asset with private send.
- [ ] Disclosure UI — a regulator portal where a court order hash is
      pasted in, multi-sig collected, Lit Action runs.
- [ ] Production deploy pipeline (probably reuses `lit-payments`
      Railway/Fly setup pattern).
- [ ] Independent security review of contract + Lit Actions.
- [ ] Legal opinion (MSB status, money-transmitter analysis) — see
      "customers" for who funds this.

### Phase 3 — GA

- [ ] Multi-currency (privEUR, privJPY).
- [ ] Mirror ledger nodes so we're not the only one holding ciphertexts.
- [ ] Programmable compliance plugins (per-jurisdiction velocity limits,
      etc.) — Lit Actions are JS, so partners can ship their own.

## How users use it

We have two user surfaces. The end-user surface is the one that has to
feel like nothing.

### End user — paying with privUSD

```
1. User goes to merchant checkout.
2. Merchant SDK pops a Lit auth flow (sign-in-with-Ethereum or
   passkey-backed PKP).
3. User clicks "Pay $250 with privUSD".
4. Browser → Lit Action `transfer`. ~3 seconds.
5. Merchant sees confirmation. Receipt has a txCommitment, no amount,
   no party data, on a public block explorer.
```

First-time users get an onboarding step: KYC + initial USDC → privUSD
mint. Subsequent payments skip both.

### End user — receiving payroll privately

```
1. Employer's payroll provider integrates the SDK once.
2. Each pay cycle, it calls `transfer` for each employee.
3. Employees see balance in their wallet, can redeem to USDC or pay
   merchants directly.
4. The employer's payroll Postgres has the numbers; the chain does
   not.
```

This is the killer wedge. Crypto-native companies pay contractors in
USDC today and every one of those contractors is doxxed to anyone who
knows their wallet address. We solve a real, present, expensive
problem.

### Issuer / partner — minting on behalf of users

If we're not the issuer of record (likely — see customers), the partner
issuer calls `mint` programmatically against their own KYC pipeline and
USDC reserve. They have their own PKP. We provide the SDK and the Lit
Action template; they own the legal entity and the customer
relationship.

### Regulator — issuing a warrant

```
1. Court order arrives at issuer.
2. Issuer files it into the disclosure portal — names account, hash of
   order, judge signature scan.
3. Disclosure-authority multi-sig (3 of 5 — issuer counsel, outside
   counsel, a designated trustee, etc.) signs.
4. Lit Action verifies, decrypts the target account/tx, encrypts result
   to regulator's pubkey.
5. Disclosure event logged on-chain for audit.
```

This is the part that has to be unimpeachable. We over-engineer the
governance here so the rest of the system can stay simple.

## Sales collateral (what BD walks in with)

Ranked by what actually closes a regulated buyer. The issuer's legal/
compliance team is the gatekeeper, not their engineers — collateral is
aimed at them.

1. **The live testnet demo** (Phase 0) — especially the disclosure reveal.
   Watching one "warrant" decrypt a single transaction while the rest stay
   dark is the thing nothing else in the market can show. This is the whole
   sale; everything below supports it.
2. **One-page compliance architecture brief** — OFAC-baked-in +
   selective-disclosure governance + reserve proof, written for compliance
   officers, not engineers. This is what survives their internal review.
3. **Reserve-proof dashboard** — `usdc.balanceOf(vault) ≥ totalSupply()`
   live. Issuers obsess over provable backing; give it to them as a link.
4. **Integration SDK + "deploy your own instance" guide** — proves *they*
   hold the keys and reserve and Lit Labs operates nothing. The legal
   posture, packaged as a sales asset.
5. **Security / trust-model doc** — TEE + threshold + CID-pinned signer, so
   their security team can diligence it without a call.
6. **The legal opinion** (Phase 1/2) — not collateral, but the gate.
   Without it, items 1–5 stall in the buyer's legal review.

## How we get customers

The product is too B2B / regulated to grow by retail TVL. We sell to
businesses that have a privacy problem with USDC today. Four wedges,
ranked by how much they'd pay:

### Wedge 1 — Crypto-native payroll (highest value, fastest close)

**Who:** Crypto cos paying remote contractors in USDC. Today every
contractor's wallet, salary, and tenure is public. This is a known
pain point — talk to any HR lead at a crypto co.

**Pitch:** "Same USDC payroll flow, contractors get private balances,
they can redeem to USDC anytime, you get a compliant audit trail."

**Channel:** Direct outbound to ~50 known crypto cos (Coinbase, Kraken,
Uniswap Labs, Optimism Foundation, etc. — Lit already has warm intros
to most). Also: payroll providers (Deel, Toku, Request Finance) — sell
to them and they distribute.

**Pricing:** 10 bps on volume, or flat $5/employee/month. The math
works at >$5M annual payroll, which most of these have.

### Wedge 2 — B2B invoice payments

**Who:** Crypto cos and fintechs settling vendor invoices in stablecoin.
Same problem as payroll, less personal, easier objection-handling.

**Pitch:** "Your vendor list is your competitive intel. Stop publishing
it."

**Channel:** Same outbound list as wedge 1, different stakeholder
(finance / AP lead, not HR).

### Wedge 3 — Stablecoin issuers as a B2B product

**Who:** New stablecoin issuers who want private-by-default as a
feature (e.g. PayPal PYUSD, Robinhood's stablecoin, banks experimenting
with deposit tokens).

**Pitch:** "License our SDK + Lit Actions. You issue. You hold the
reserve. We give you the privacy layer." We don't operate the
stablecoin; we sell the infrastructure to people who do.

**Channel:** This is the strategically biggest customer if it lands.
Probably one BD person dedicated to it. Lit already has some banking
conversations; ride those.

**Pricing:** Big upfront integration fee ($250k–$1M), plus a per-tx
fee paid in their token.

### Wedge 4 — Existing privacy-curious projects

**Who:** Aztec, Penumbra, Railgun users who want a stablecoin but don't
want to build the compliance side. Smaller TAM, but very loud
advocates, good for credibility.

**Pitch:** "We're the compliant private dollar your users actually
want."

**Channel:** Crypto Twitter, ETHGlobal hackathons, podcast tour. This
is the marketing wedge, not the revenue wedge.

### Sequencing

1. Build phase 1, pilot with **one** crypto co's payroll (warm intro
   target: ask Lit's existing close partners; we probably have 3 good
   candidates).
2. Use that case study to close 5 more payroll pilots.
3. With $10M+ TVL and a clean compliance story, approach a stablecoin
   issuer for wedge 3.
4. Wedge 4 runs in parallel as PR / dev-credibility.

### What blocks growth

- **Legal opinion:** until we have a clean MSB / money-transmitter
  analysis (probably says "as long as we're infrastructure and the
  issuer is the licensed entity, we're fine"), enterprise pilots will
  stall in their legal review. Get this done in phase 2.
- **One real disclosure event:** the first warrant we process is
  marketing gold. We should plan for it (have outside counsel
  pre-briefed, comms plan ready) rather than be surprised.
- **Lit network reliability:** payroll cannot fail. SLA needs to be
  better than what we offer dapp customers today. Phase 2 includes a
  hot standby story.

## Open questions

Settled (moved to "Decisions locked in"): issue-vs-infrastructure
(infrastructure), storage location (on-chain commitment/nullifier, no
off-chain ledger), OFAC posture (baked in). Remaining:

- Base or another L2? Base for compliance story (Coinbase) + cheapest fees.
  Revisit for issuer partners who prefer a different chain.
- Can `shieldedTransfer` latency stay under 3s at p99 with note decryption +
  OFAC `eth_call` inside the action? **Validate during Phase 0** — it's the
  cheapest place to find out, and it's a promise payroll customers will hold
  us to.
- Note-scanning UX: how does a fresh client reconstruct its balance fast
  without an indexer it has to trust? Acceptable to ship an optional caching
  indexer as long as the trustless path exists.
- Commitment/nullifier storage growth / gas: each note writes a new mapping
  slot on Base — model the cost per transfer at scale before quoting issuers
  a per-tx price.

## Decision needed before starting

1. Approval to build **Phase 0 (the demo)** — ~1–2 weeks of one engineer.
   This is small, high-leverage, and de-risks everything downstream. Don't
   gate it on the larger staffing decision.
2. Then: allocate one engineer + one BD person for ~6 weeks to Phase 1.
3. Pick the pilot partner from Lit's existing warm relationships.
4. Engage a fintech attorney (Storm-verdict-literate) for the regulatory
   opinion in parallel with Phase 1 build — don't block on it, but don't
   ship to a paying issuer without it.
