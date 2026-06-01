# Lit Solver Vault — Demo & Sales Plan

**Status:** draft for discussion
**Owner:** chris@litprotocol.com
**Goal:** a runnable demo + pitch artifacts we can put in front of solver/filler teams to win pilots.

## One-line pitch

**Lit Solver Vault** — programmable, policy-gated key custody for intent-system solvers and fillers. The signing key lives inside Lit (PKP + Lit Action policy), not on the solver's box, so a compromised bot can't drain inventory. Inventory itself lives in a per-chain vault contract the solver controls — Lit guards operations, the solver's Safe guards the money, and Lit can never block the solver from their funds.

## Who this is for

Primary targets, in order of pitch-readiness:

1. **ERC-7683 / Across fillers** — multi-chain by construction, latency-tolerant, security-conscious. Best fit. Phase 5 integrates against Across testnet directly.
2. **UniswapX fillers** — long tail of independent operators with uneven security maturity. Volume play.
3. **Bridge relayers / executors** (LayerZero, Wormhole-adjacent, Hyperlane) — same key-custody shape, larger TVL per customer.
4. **CoW solvers / 1inch resolvers** — bigger logos, longer sales cycles; pitch after we have a reference customer.

Explicitly **not** the first target: Hashflow-tier RFQ market makers and anything HFT-shaped. Lit's sign path is ~220 ms — fine for intent settlement, not for sub-100 ms quoting.

## Why a solver buys this

- **One compromised box ≠ drained inventory.** No raw key on disk anywhere.
- **One key abstraction across all chains.** Today solvers juggle separate hot wallets per chain/venue; one Lit PKP signs on any chain Lit supports.
- **Programmable guardrails that survive ops turnover:** notional caps, per-chain caps, allowlisted intent contracts, allowlisted settlement recipients, slippage bounds, time-of-day windows, kill switch.
- **Delegation without custody handoff.** Ops engineers, on-call, partner MMs can be scoped to sign without ever touching the key.
- **Audit trail.** Every signature request + policy decision logged for compliance and LP reporting.
- **LP-funded inventory.** An institutional LP funds a solver's pool with Lit-enforced spend constraints; the solver operates but can never rug. The vault contract makes this natural rather than aspirational — see the LP-funded section below.

## "Why don't people just do this on-chain?" — the central objection

There's a serious on-chain alternative: Safe + Zodiac roles modifier, or a custom executor contract with policy, plus 4337 session keys. For purely on-chain intent settlement on a single chain, it's competitive. Lit wins when several of these are true, and for real solvers several always are:

1. **Multi-chain.** Smart contract wallets are per-chain — separate deploy, separate address, separate policy state. Solvers operate across 5–10 chains. One Lit PKP signs on all of them with one source-of-truth policy.
2. **On-chain policy bounds the blast radius; it doesn't prevent unauthorized signing.** The Safe says "operator EOA `0xabc` can submit fills within these limits." `0xabc` is still a hot key on a box. Compromise it and the attacker signs every fill the policy allows. With Lit there is **no key on the box** — the attacker has to ask Lit, and Lit applies policy at signing time. Bound vs. prevented.
3. **Solvers sign off-chain too.** UniswapX orders, RFQ quotes, bridge attestations, intent acks. On-chain policy doesn't help there; the EOA still has to live somewhere. Lit gates off-chain signing the same way.
4. **Policy can use off-chain inputs.** Internal risk scores, treasury caps, expensive oracle reads. This logic does not want to live in a public Solidity contract.
5. **Privacy.** On-chain policy is public. Competitors see your caps, allowlists, rate limits.
6. **Gas.** Complex on-chain policy charges gas every fill, every chain, forever.

**Crisp one-liner for the room:** *on-chain custody bounds what a compromise can do; Lit prevents the compromise from signing in the first place — and does it once across every chain the solver operates on.*

## Architecture

```
                      ┌─────────────────────────────┐
Solver bot ──sign──▶  │  Lit Network                │
                      │  • Lit Action (policy)      │
                      │  • threshold sign           │
                      └──────────┬──────────────────┘
                                 │ sig
                                 ▼
                  Solver bot submits tx on-chain
                                 │
                                 ▼
                      ┌─────────────────────────────┐
                      │  SolverVault.sol  (per chain)│
                      │  • holds inventory          │
                      │  • normal ops: Lit-signed   │
                      │  • exit(): local key → Safe │
                      └─────────────────────────────┘
```

The bot has **only** a Lit auth credential. No raw EVM private key on disk. Inventory lives in `SolverVault` contracts (one per chain), not in an EOA the bot controls.

## Policy (Lit Action) — what it actually checks

For the demo, keep it tight and obvious:

1. `payload.kind == "fill"` — refuse anything else.
2. `payload.settlement_contract ∈ ALLOWLIST` — Across settlement (testnet) plus a mock.
3. `payload.recipient == derived_from_intent(payload.intent)` — recipient must match the order's settlement address. This is the kill shot for exfiltration.
4. `payload.notional_usd <= POLICY.max_notional_usd` — current cap.
5. `payload.chain_id ∈ POLICY.allowed_chains`.
6. `now() - last_sig_ts >= POLICY.min_interval_ms` — basic rate limit.
7. `POLICY.kill_switch == false` — global stop.

Policy state (caps, kill switch) lives in a small Postgres table that the dashboard writes and the Lit Action reads via a signed config blob. Stretch goal: move policy state on-chain so the dashboard is just a UI on a contract.

## SolverVault — inventory custody and emergency exit

A minimal contract, ~150 lines of Solidity, deployable as a clone-factory per solver and reused on every chain.

**Two privileged paths:**

- **Normal operation.** Lit-signed call routes funds out as part of a fill (or any operation policy allows).
- **Emergency exit.** Local "owner" key calls `exit()` at any time. Funds sweep to a **pinned destination** — recommended to be a Safe multisig the customer already runs for treasury. No timer. No Lit veto.

**Why no veto** (we went around on this — the answer landed clearly): the Safe destination already makes evil exit a vanishingly small risk (attacker would need the local key *plus* 2-of-N hardware-held Safe signers). Adding a Lit veto introduces a real failure mode where a half-working Lit could leave funds trapped, and it weakens the pitch from *"your funds are yours, period"* to *"your funds are yours unless Lit decides otherwise."* Wrong tradeoff.

**The thing that actually matters: `setColdWallet()`.** The pinned destination is the only thing protecting funds from a combined attack, so destination changes are the protected path, not exits. Two options for customers:

- **Lit-signed + 7-day timelock.** Even if Lit is fully compromised, customer has a week to detect via on-chain monitoring and intervene out-of-band.
- **Co-signed by customer's Safe.** Heavier UX, paranoia-grade. Recommended for LP-funded vaults.

**Mental model:** *exit is fast; destination changes are slow.* Anyone who can press exit can only push funds where the customer already approved them to go.

### LP-funded inventory (the bigger product)

The vault makes LP-financed solving natural:

- LP deposits into the vault.
- Vault enforces: funds leave **only** via Lit-policy-gated fills, **or** via exit to the pinned destination Safe.
- The destination Safe is **co-signed by the LP** (or is a contract requiring LP approval). LP trust collapses to "do I trust the destination" instead of "do I trust the solver's whole ops stack."

This isn't v1 of the demo, but it's the bigger pitch and worth a section in the one-pager. The vault contract was always going to enable this; the design above makes it natural.

### Tradeoffs to be honest about

- **Per-chain vault deploys.** We re-introduce some on-chain pain — but only for a dumb audit-once-deploy-everywhere contract, not for policy. Policy still lives in Lit, identical across chains.
- **Settlement compatibility.** Inventory now lives in a contract, not an EOA. Across, UniswapX, ERC-7683 all handle contract fillers cleanly by spec, but **Phase 1 must verify Across testnet specifically** — first thing a prospect's engineer will check.
- **Gas for exit.** Local key needs a small gas-paying EOA. $50 sitting on it, refilled periodically.
- **Audit cost.** Real contract holding real inventory needs a real audit before any prospect goes to mainnet. Not before the demo, but on the path to first paid pilot. Budget item.

## Liveness story for the pitch

State it explicitly, hour one:

- **Lit outage = you stop earning, not stuck capital.** Solver bot gracefully stops filling; no missed-fill capital is at risk.
- **Inventory is always recoverable.** Local `exit()` works regardless of Lit state. Funds sweep to the customer's Safe.
- **The only Lit-dependent path is changing the destination**, which is timelocked or multi-sig co-signed. Even a fully compromised Lit cannot trap a customer's funds — only delay a destination change.

Pitch line: *"Lit guards your operations; your Safe guards your inventory. Lit can never block you from your money."*

## Pricing

- **Pilot: free.** Standard. Removes price as a sales objection during validation.
- **After pilot: $0.01 per signature.** Same as Lit's standard signature pricing — the value framing is "your fills already cost gas; this is a small fixed component for security and policy enforcement."
- **Enterprise:** prebought signature allocations at a discount. Lets large solvers convert hot-key risk into a predictable line item.

For the one-pager, lead with *"free during pilot; volume pricing after."* Don't put the per-sig number on the page — keep that for the call.

## Latency

- **Quote 220 ms end-to-end** as the current Lit signing latency.
- **Measure live on mainnet during demo build** and put the measured number in the one-pager rather than the quoted figure. Truth-in-advertising — and "we measured X" is more convincing than "we estimate X."
- Add a small latency-budget callout: 220 ms is invisible inside intent-settlement timescales (block times of seconds), and even a 2× regression would still be invisible.

## What we're building

A complete reference implementation — repo `lit-solver-vault` — that we own end-to-end (solver bot, Lit Actions, vault contracts, dashboard, docs). Small enough to read in an afternoon, real enough to fill an actual order on Across testnet, and shaped so a prospect can fork it and be running in a day. We shop this around as a working example, not a slide deck.

### Repo layout

```
lit-solver-vault/
├── solver-bot/            TS bot that fills a real Across testnet intent; signs via Lit
├── lit-actions/
│   ├── solver-policy.ts   the main policy gate
│   ├── admin.ts           kill switch, policy version bumps
│   └── README.md          policy authoring guide
├── attacker/
│   ├── exfiltrate.ts      try to sign a transfer to attacker addr
│   └── bad-fill.ts        try to sign a malformed intent fill
├── contracts/
│   ├── SolverVault.sol    inventory custody + exit
│   ├── VaultFactory.sol   clone-factory deploy
│   └── test/              foundry tests
├── dashboard/             Next.js: live sigs, policy version, kill switch, vault status
├── docs/
│   ├── ARCHITECTURE.md
│   ├── DEMO-SCRIPT.md     the 5-min live walkthrough
│   ├── PITCH.md           one-pager source
│   ├── THREAT-MODEL.md    "without Lit / with Lit" attack-vector table
│   └── FAQ.md             on-chain alternative, liveness, etc.
└── README.md
```

## Demo script (5 minutes)

1. **Cold start.** `grep -r PRIVATE_KEY solver-bot/` — empty. Show env: just a Lit session credential. Show vault address holding real testnet inventory.
2. **Happy path.** A real Across testnet intent lands. Bot calls Lit. Policy passes. Vault releases funds, fill tx lands. Dashboard logs the signature with policy decision and latency in ms.
3. **Attack 1 — exfiltration.** Run `attacker/exfiltrate.ts`. It tries to sign a transfer to an attacker address. Lit Action rejects. Show the rejection in the dashboard with reason code.
4. **Attack 2 — bad fill.** Malformed intent (recipient ≠ order settlement). Rejected.
5. **Live policy update.** Drop `max_notional_usd` from $1M to $100k via the dashboard. No key rotation, no downtime. Replay a $500k fill — now rejected.
6. **Kill switch.** Flip it. Everything stops. Flip back.
7. **Emergency exit.** Pretend Lit is down. Call `exit()` from the local key. Funds sweep to the registered Safe. *"Lit can never trap your money."*

## Sales artifacts

- **One-pager PDF** (`docs/PITCH.md` rendered).
- **Loom of the demo** for cold outbound without scheduling a call.
- **Threat-model table** (`docs/THREAT-MODEL.md`) — "without Lit / with Lit" for the 6 most common solver attack vectors: compromised cloud box, malicious ops insider, leaked deploy creds, supply-chain dependency, LP rug, accidental fat-finger.
- **FAQ** (`docs/FAQ.md`) — answers to the on-chain alternative question, liveness/exit story, latency, policy-as-code risk.
- **Reference policy templates** — one per target intent system (UniswapX, Across, ERC-7683).
- **Prospect spreadsheet** — all known solvers/fillers across the 4 target categories with a "warm intro path?" column to drive outbound work.

## Build sequencing

Roughly 2.5–3 weeks of focused work with Across testnet integration in-scope from the start.

### Phase 1 — skeleton + happy path on Across testnet (4-5 days)

- Repo scaffold, README, ARCHITECTURE.md.
- **Verify Across testnet accepts contract fillers cleanly** before going further. This is the gate.
- `SolverVault.sol` v1 + foundry tests for `exit()` and `setColdWallet()` timelock.
- Solver bot signs a hardcoded fill payload via Lit, lands a real Across testnet tx via the vault.
- Minimal Lit Action: notional cap + allowlist + kill switch.
- Measure and record real Lit mainnet signing latency.
- **Checkpoint:** internal walkthrough, gut-check the bones, confirm latency number.

### Phase 2 — attacks + policy depth (3-4 days)

- Exfiltration and bad-fill attacker scripts.
- Policy gets recipient-binding, rate limit.
- Decision log written somewhere queryable.
- **Checkpoint:** can we tell the security story without hand-waving?

### Phase 3 — dashboard + emergency exit (3-4 days)

- Next.js dashboard: live signatures, policy editor, kill switch toggle, vault status.
- Live policy updates without key rotation.
- Wire `exit()` into the demo flow with the Safe destination.
- **Checkpoint:** demo-ready end-to-end including step 7 of the demo script.

### Phase 4 — sales artifacts (3-4 days)

- Record Loom.
- Write one-pager, threat-model, FAQ.
- Stub policy templates for UniswapX and ERC-7683 (Across is real).
- Build prospect spreadsheet across the 4 categories; flag warm intro paths (mutual investors are the likely vector).

### Path to first paid pilot (after demo lands)

- **SolverVault audit.** Real money on the line — get this done before mainnet. Budget line item.
- **Reference policy hardening** for the first signed pilot's intent system.
- **Operational alerting** — paging on rejected sigs, on policy version drift, on vault state changes.

## Open questions remaining

1. **Prospect list.** We need to build the spreadsheet and figure out which solvers we already know or can reach via mutual investors. Drives whether the first paid pilot is Across-shaped or something else.
2. **Audit firm + budget.** Vault audit is on the critical path to first paid pilot. Worth picking the firm now so we're not scrambling later.

## Out of scope for the demo

- Production-grade dashboard auth (magic link is fine for the demo).
- Multi-tenant control plane — each prospect would self-host or get a dedicated instance during pilot.
- Non-EVM chains. Add after first EVM pilot lands.
- On-chain policy state. Postgres-backed signed blob is fine for v1.
- Vault audit (deferred to path-to-pilot, not demo).
- LP-funded vault variant. Pitched in the one-pager, built when a first LP-shaped customer shows up.
