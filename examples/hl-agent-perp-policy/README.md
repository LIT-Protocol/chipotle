# Hyperliquid Agent Perp Policy

**Perp orders on Hyperliquid placed by a trading key that never exists
anywhere — not in an env file, not in a database, not even as a sealed
secret. The key is derived from the Lit Action's own CID inside the TEE; the
venue knows it only as an approved agent wallet, which Hyperliquid
structurally bars from withdrawing. The action fences every order with policy
(coin allowlist, max leverage, notional cap, reduce-only) before it signs.**
This is the PKP-native showcase from plan D8.

## The "no credential exists anywhere" story

CEX connectors seal an existing API secret; a leak is survivable but
possible. Here there is nothing to seal:

- **The key is the code.** `Lit.Actions.getLitActionPrivateKey()` derives a
  secp256k1 key from the action's IPFS CID — which covers the lit-venues
  bundle *and* the policy source, `POLICY` constants included. The key never
  leaves the TEE; the only thing that can ever sign with it is this exact
  code. Change a byte and it's a different key (re-run `approve-agent`).
- **The venue enforces the custody boundary.** The user's master account
  signs one `approveAgent` for the agent's address. Agents on Hyperliquid can
  sign orders and cancels but **never withdrawals or transfers** — that's the
  venue's rule, not just our policy. Funds stay custodied under the master.
- **Revocation is the user's, always.** The master can revoke the named agent
  (`lit-policy`) venue-side at any time, or simply move funds. On the Lit
  side, removing the CID from the permission group kills execution too.

```
 user's master key (.env, used once)             Lit TEE                       Hyperliquid testnet
        │                                           │                                  │
        │ approve-agent.js: "address"? ────────────►│ key = f(action CID)              │
        │◄── 0xAGENT ───────────────────────────────┤ (key never leaves)               │
        │                                           │                                  │
        │ approveAgent(0xAGENT) — signed by MASTER ────────────────────────────────────► agent may trade,
        │                                           │                                  │ NEVER withdraw
        │ trade.js: side/amount/coin/... ──────────►│ fences: allowlist, leverage ≤ 3, │
        │                                           │ notional ≤ $1000, reduce-only    │
        │                                           │ then EIP-712-sign as the agent ─►│
        │◄── {order, positions} ────────────────────┤                                  │
```

## The policy fences

Enforced inside the action, before anything is signed — and bound into the
CID, so weakening a fence changes the agent address itself:

- **Coin allowlist:** `ETH` and `BTC` perps only.
- **Max leverage 3×** (cross): requested leverage is clamped via `setLeverage`.
- **Notional cap $1000** on any exposure-increasing order and on the
  resulting position. Reduce-only orders bypass the cap — they can only
  shrink exposure, and the venue enforces that semantics.
- **Reduce-only fencing:** explicit `reduce-only` is honored; an order that
  opposes your position but would overshoot past flat into a breach is forced
  reduce-only (it may close, never flip); same-direction growth past the cap
  is refused outright.
- **Testnet only as written:** `sandbox: true` is part of the hashed source.

## Files

| Path | Purpose |
| --- | --- |
| `action/perpPolicy.js` | The Lit Action. `"address"` branch returns the CID-bound agent address; `"trade"` branch enforces the fences and places the order via `LitVenues` (hyperliquid, `pkp-eip712`). |
| `scripts/_lit.js` | Concatenates the prebuilt `lit-venues` IIFE bundle (~175 KB) above the action source and runs it via `/lit_action`. The CID — and the agent key — covers exactly these bytes. |
| `scripts/_env.js` | Minimal `.env` reader / upserter, inlined so the folder is self-contained. |
| `scripts/setup.js` | One-shot: CID, permission group, scoped usage key, derive + record the agent address, register the action. |
| `scripts/approve-agent.js` | Run by the user: re-derives the agent address live, then signs `approveAgent` with the master key from Node (lit-venues ESM build) — the master key's only job. |
| `scripts/trade.js` | Sends a trade request (with a fresh `cloid`) to the policy action and prints the order + positions. |

## Walkthrough

### 1. Install + configure

```bash
cp .env.example .env
npm install
```

Set `LIT_API_KEY` (account-level key) and `HL_MASTER_PRIVATE_KEY` — a
**throwaway testnet** key (generator one-liner in `.env.example`).
The scripts use the sibling `../../lit-venues/dist/` builds directly
(committed; rebuild with `npm run build` in `lit-venues/` if needed) — no npm
dependency.

### 2. Fund the testnet master

Connect the master wallet at <https://app.hyperliquid-testnet.xyz> and claim
mock-USDC from the faucet (`/drip`). **This demo needs a funded master:**
Hyperliquid rejects `approveAgent` (and everything else) from accounts it has
never seen funds for.

### 3. Setup + approve the agent

```bash
npm run setup          # CID, group, usage key, agent address (no key created!)
npm run approve-agent  # master signs the one-time trade-only grant
```

### 4. Trade inside the fences

```bash
npm run trade -- buy 0.01 ETH              # market order, 1x
npm run trade -- buy 0.01 ETH 2000 3x      # GTC limit @ 2000, 3x (the cap)
npm run trade -- sell 0.01 ETH reduce-only # close, may only shrink
npm run trade -- buy 1 BTC                 # refused: over the $1000 notional cap
npm run trade -- buy 0.01 ETH 2000 9x      # placed, but clamped to 3x
```

Each run prints the applied policy, the order, and the resulting positions;
verify against the testnet app. Client order ids (`cloid`) are generated per
attempt and must match `/^0x[0-9a-f]{32}$/`.

## Production notes

- **Geo posture.** Hyperliquid's ToS bars US persons and the app geofences by
  IP; whether mainnet API egress needs the D4 proxy is measured at the M2.5
  gate. This example is testnet-only by construction.
- **One agent per connection.** Hyperliquid nonces are ms-timestamps per
  signer — don't run concurrent trades through the same agent.
- **Per-user fencing.** Stamp a user id (or their master address) into the
  action source and each user gets their own CID → own agent key → own
  venue-side approval, with the policy bound to it.
- **PKP-as-master** is the other D8 mode (the action key *is* the account);
  sweeps then go through the email-approval primitive — see
  [`cex-sweep-with-email-approval`](../cex-sweep-with-email-approval).
