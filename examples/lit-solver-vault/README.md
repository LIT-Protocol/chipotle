# Lit Solver Vault

**Policy-gated key custody for intent-system solvers and fillers. The key that
moves inventory lives in a Lit Action, not on the solver's box — so a
compromised bot can't drain the vault, and operational guardrails are enforced
at signing time.**

Solvers and fillers (UniswapX, Across, CoW, 1inch Fusion, ERC-7683, bridge
relayers) run a bot that holds a hot key and signs fills against an inventory
balance. Compromise the box, drain the inventory. That's the threat this
example removes.

Here the inventory lives in a `SolverVault` contract. The only signature that
releases a fill comes from a **Lit Action that *is* the policy** — it screens
every fill (recipient binding, notional cap, settlement allowlist, kill switch)
and signs only when it passes. The solver bot never holds that key. It can
*ask* Lit to authorize a fill; it can't authorize one itself.

> This example is the runnable backbone of the plan in
> [`plans/lit-solver-custody-demo.md`](../../plans/lit-solver-custody-demo.md).
> It covers the contracts, the policy action, and the attack/exit demo scripts
> (plan phases 1–2 + the exit story). The live dashboard (plan phase 3) and the
> real Across-testnet integration (plan phase 5) are noted as next steps below.

## How it works

```
   solver bot          Lit Action            Alchemy (Base Sepolia)        SolverVault
   (usage key)        (solverPolicy)        vault cfg + settlement order
       │                   │                         │                        │
       │ js_params         │                         │                        │
       ├──────────────────►│                         │                        │
       │                   │ check rpcUrl host       │                        │
       │                   │                         │                        │
       │                   │ read killSwitch,        │                        │
       │                   │ maxFillAmount,          │                        │
       │                   │ allowedSettlement       │                        │
       │                   ├────────────────────────►│                        │
       │                   │◄────────────────────────┤                        │
       │                   │ read getOrder(depositId)│                        │
       │                   ├────────────────────────►│                        │
       │                   │◄────────────────────────┤                        │
       │                   │ recipient == order? cap?│                        │
       │ sig / reason      │ if ok: sign with        │                        │
       │◄──────────────────┤ getLitActionPrivateKey()│                        │
       │                                                                      │
       │ executeFill(token, recipient, amount, nonce, deadline, sig) ────────►│
       │                                              recover(sig)==policySigner ✓
       │                                              transfer to recipient
```

### Why this is a Lit-shaped problem

You could put this entirely on-chain — a Safe with a Zodiac roles modifier, or
a custom executor contract, plus a 4337 session key. For single-chain,
fully-on-chain settlement, that's a fine answer.

Lit wins where solvers actually live:

- **Multi-chain with one policy.** A smart-contract wallet is per-chain —
  separate deploy, address, and policy state to keep in sync across the 5–10
  chains a real filler operates on. One Lit Action is one source of truth that
  signs for vaults on every chain.
- **It prevents unauthorized signing, not just bounds it.** On-chain policy
  says "operator EOA `0xabc` may submit fills within these limits" — but
  `0xabc` is still a hot key on a box, and a compromise can sign every fill the
  policy allows. Here there is *no key on the box*; the attacker has to ask
  Lit, and Lit applies policy before it signs.
- **Off-chain inputs and privacy.** The policy can read private risk scores or
  use logic you don't want to publish as Solidity. (This demo keeps config
  on-chain for simplicity — see *Production hardening*.)

The signature comes from `Lit.Actions.getLitActionPrivateKey()` — a key derived
deterministically from the action's IPFS CID. The deployed `SolverVault` pins
that address as its `policySigner`. **Edit the policy by a byte and the CID, key,
and address all change — so the vault stops honoring the modified policy
automatically.** The policy can't be silently swapped out from under the money.

## Trust model

Four roles, deliberately separated:

| Role | Holds | Can do | Can't do |
| --- | --- | --- | --- |
| **solver bot** | a scoped Lit usage key | *request* a fill authorization | move inventory; change policy; exit |
| **policySigner** | CID-derived key, inside Lit | authorize good fills, refuse bad ones | be edited without changing its own address |
| **owner** | a local key (recommend a Safe) | restrict policy (kill switch, lower cap); `exit` to cold wallet | redirect a fill; change the exit destination quickly |
| **coldWallet** | — | receive emergency exits | — |

The headline property: **a compromised solver bot** (attacker has the usage key
*and* the box) can ask for fills, but the policy action refuses anything that
doesn't match a real on-chain order under the cap. No signature, no movement.
To actually steal you'd need to compromise the `owner` key too — and even then
you can only sweep to the pinned `coldWallet`.

The anti-exfiltration check is the load-bearing one: the action reads the order
from the settlement contract on-chain and binds the fill to *that* recipient.
The bot can put any recipient in `js_params`, but it can't rewrite the order, so
the mismatch is caught.

## Files

| Path | Purpose |
| --- | --- |
| `action/solverPolicy.js` | The policy Lit Action: reads vault config + the on-chain order, enforces recipient binding / cap / allowlist / kill switch, signs the fill authorization. |
| `contracts/SolverVault.sol` | Inventory custody. `executeFill` verifies a policy signature; `exit` sweeps to the cold wallet; cold-wallet changes are timelocked. |
| `contracts/MockSettlement.sol` | Stand-in intent/order book. Holds the canonical `(recipient, token, amount)` per order — the trust anchor the action reads. Swap for a real settlement contract to integrate. |
| `contracts/MockUSDC.sol` | 6-decimal faucet token used as inventory. |
| `scripts/setup.js` | One-shot: computes the action CID, derives `policySigner`, creates + wires the group/usage key, deploys + funds everything. |
| `scripts/deploy.js` | Hardhat deploy: mocks + vault, funds the vault, allowlists the settlement, posts a sample order. |
| `scripts/fill.js` | Happy path: authorize a legit fill and submit `executeFill`. |
| `scripts/attack-exfiltrate.js` | Compromised bot tries to redirect a fill → policy rejects. |
| `scripts/attack-bad-fill.js` | Compromised bot tries an over-cap fill → policy rejects. |
| `scripts/set-policy.js` | Live policy update (lower cap, toggle kill switch) — no key rotation. |
| `scripts/exit.js` | Emergency sweep to the cold wallet, no Lit involved. |
| `scripts/_lit.js` / `scripts/_env.js` | Shared helpers (call the action; read/upsert `.env`). |

## Walkthrough

### 1. Fill in your inputs

```bash
cp .env.example .env
npm install
```

Edit `.env` and set:
- `LIT_API_KEY` — your **account-level (master) API key** from the
  [Chipotle dashboard](https://dashboard.chipotle.litprotocol.com), *not* a
  scoped usage key. Setup calls management endpoints (`/add_action`,
  `/add_group`) that revert `NotMasterAccount` on scoped keys.
- `ALCHEMY_BASE_SEPOLIA_URL` — a **Base-Sepolia Alchemy URL** of the form
  `https://base-sepolia.g.alchemy.com/v2/<your-api-key>`. The action hardcodes a
  hostname whitelist requiring `base-sepolia.g.alchemy.com` (see *Trust anchor*
  below). This URL is reused as the network RPC for deploys/txs.
- `DEPLOYER_PRIVATE_KEY` — an EOA with Base-Sepolia gas. It deploys the
  contracts and becomes the vault `owner`.
- `SOLVER_PRIVATE_KEY` — the EOA the bot uses to submit fill txs (can be the
  same as the deployer for testing). Holds no inventory.

### Trust anchor

A caller can pass any `rpcUrl` to the action, so the action checks
`new URL(rpcUrl).hostname` against an anchored regex
(`/^base-sepolia\.g\.alchemy\.com$/i`). The check passes only when TLS delivers
data from Alchemy's actual servers. To swap providers or chains, edit
`ALLOWED_RPC_HOST` in [`action/solverPolicy.js`](./action/solverPolicy.js) —
which changes the action's CID and therefore its `policySigner` address, so
you'd redeploy the vault (or add a multisig-gated setter to rotate it).

### 2. Run setup

```bash
npm run setup
```

Seven steps, printed as they go:

1. Compute the action's IPFS CID.
2. Create a permission group with a **wildcard action allowlist**
   (`cid_hashes_permitted: ["0"]`) — what makes the one-shot deriver in step 4
   executable.
3. Create a scoped usage API key with `execute_in_groups: [groupId]`, saved as
   `LIT_USAGE_API_KEY`. The bot uses this; the master key can't execute actions
   in your own groups.
4. Derive the action's wallet address — the vault's `policySigner`.
5. Register the action against your account (metadata).
6. Add the action CID to the group (audit trail).
7. Deploy + wire the contracts (`scripts/deploy.js`): MockUSDC, MockSettlement,
   SolverVault (pinning `policySigner`, `owner`, `coldWallet`), fund the vault
   with 100,000 mUSDC, allowlist the settlement, post a sample 100-mUSDC order.

Re-running does a fresh setup top-to-bottom and orphans the previous
group/usage key/contracts — the simplest reset for a docs example.

### 3. Walk the demo

```bash
# Cold start — there is no inventory-moving key in the bot's environment:
grep -ri "PRIVATE_KEY" action/        # nothing — the action signs with a CID-derived key

# Happy path: a legit order clears policy and the fill lands.
npm run fill

# Attack 1 — exfiltration. Compromised bot tries to pay the attacker. Rejected.
npm run attack:exfiltrate

# Attack 2 — over-cap fill. Rejected.
npm run attack:bad-fill

# Live policy update: lower the per-fill cap to 50 mUSDC. No key rotation.
npm run policy -- --max 50
npm run fill            # the same 100-mUSDC fill is now rejected by the new cap

# Kill switch: stop everything, then release it.
npm run policy -- --kill on
npm run fill            # rejected: kill switch engaged
npm run policy -- --kill off

# Emergency exit: pretend Lit is down. Owner sweeps inventory to the cold wallet.
npm run exit
```

`npm run fill` prints the policy authorization latency in milliseconds. The
action fires its on-chain reads concurrently (`Promise.all`), so the full
authorization round-trip — reads plus threshold sign — measured **~335 ms warm**
against the live Across path (see the Across section for the numbers and the
before/after of parallelizing). Pure threshold signing is a fraction of that.

## Liveness & exit

The pitch line is *"Lit guards your operations; your Safe guards your
inventory — Lit can never block you from your money."* The contract earns it:

- **Lit outage = stop earning, not stuck capital.** No fills can be authorized,
  but `exit(token)` works any time and needs no Lit.
- **The exit destination is pinned.** Even a compromised `owner` key can only
  push funds to `coldWallet`. Changing `coldWallet` is the slow, protected path
  — `requestColdWalletChange` then `commitColdWalletChange` after a 7-day
  timelock (`COLD_WALLET_TIMELOCK`).

We deliberately did **not** add a "Lit can veto the exit" mechanism: a
half-working Lit that still vetoes could trap funds, and "your funds are yours,
period" is a strictly better promise than "unless Lit decides otherwise."

## Production hardening

- **Make `owner` a Safe.** The demo uses the deployer EOA. In production the
  owner and the cold wallet should both be multisigs you already run.
- **Move policy config off-chain.** This demo stores `killSwitch` /
  `maxFillAmount` / `allowedSettlement` on the vault so updates are one tx and
  the action reads them with a plain `eth_call`. That's public — it leaks your
  caps and allowlist to competitors. In production keep config in a signed
  off-chain blob the action verifies, or behind a private endpoint.
- **Bind to the real settlement contract.** Replace `MockSettlement` with the
  actual order/deposit accessor of your intent system. For **Across**, read the
  deposit from the SpokePool and have `executeFill` call `fillRelay` instead of
  a bare `transfer`; the recipient-binding logic is the same shape. (This is
  plan phase 5.)
- **Per-chain caps and rate limits.** Add a `min_interval` and per-chain cap to
  the policy for production guardrails beyond the single notional cap shown here.
- **Audit.** `SolverVault` holds real inventory — it needs an audit before any
  mainnet deployment. It is unaudited here.
- **Settlement compatibility.** Inventory lives in a contract, not an EOA.
  Across / UniswapX / ERC-7683 all support contract fillers, but verify your
  target system's `msg.sender` assumptions before mainnet.

## Across testnet integration (the real-fill variant)

The mock demo above runs with zero external dependencies — great for the
security story and CI. The Across variant proves it on a live intent system:
the vault acts as a real Across **relayer** (filler), filling an actual
cross-chain deposit on testnet.

**Route (confirmed enabled):** Sepolia → Base Sepolia, WETH both sides.

| | |
| --- | --- |
| Origin | Sepolia (11155111), SpokePool `0x5ef6C01E11889d86803e0B23e3cB3F9E9d97B662` |
| Destination | Base Sepolia (84532), SpokePool `0x82B564983aE7274c86695917BBf8C99ECb6F0F8F` |
| Token | WETH (`0xfFf9…6B14` → `0x4200…0006`), wrap ETH, no faucet |

### Why the policy matters *more* here

On the destination chain, `fillV3Relay` does **not** check the fill against the
origin deposit — Across reconciles fills to deposits later, at reimbursement
time. So nothing on-chain stops a compromised relayer from filling to its own
address and just eating the loss. The protection that prevents that is the Lit
policy: `acrossPolicy.js` reads the real `FundsDeposited` event on the origin
chain and **reconstructs the entire relay from the deposit** — there's no
caller-supplied recipient to tamper with, so the only relay it will ever sign
pays the deposit's real recipient.

This has been run live: deposit on Sepolia → Lit authorizes → `AcrossSolverVault`
fills on Base Sepolia, paying the recipient from inventory; and the attack
script confirms the only relay Lit signs pays the real recipient while a forged
direct fill reverts `InvalidPolicySignature`.

### Pieces

| Path | Purpose |
| --- | --- |
| `contracts/AcrossSolverVault.sol` | Holds WETH inventory; `executeAcrossFill` verifies a policy sig over the full relay, then approves + calls `SpokePool.fillV3Relay`. Same owner/exit/cold-wallet machinery as `SolverVault`. |
| `action/acrossPolicy.js` | Reads the deposit via `eth_getLogs` on the origin chain, reconstructs + binds the relay, enforces cap / kill switch / origin-chain allowlist, signs. |
| `scripts/across-deposit.js` | Plays the depositor: wraps ETH, calls `depositV3` naming our vault as the exclusive relayer for 30 min (so a public testnet relayer can't snipe the deposit before we fill it). |
| `scripts/across-fill.js` | The relayer happy path: authorize via Lit, submit `executeAcrossFill`. |
| `scripts/across-attack.js` | Shows exfiltration is impossible (signed relay always pays the real recipient; forged direct fill reverts). |
| `scripts/setup-across.js` / `deploy-across.js` | Lit wiring + deploy/fund the vault. |

### Run it

Add to `.env` (see the Across block in `.env.example`):
- `ALCHEMY_ETH_SEPOLIA_URL` — a Sepolia Alchemy URL (whitelisted by the action).
- Make sure `DEPLOYER_PRIVATE_KEY` holds a little **Sepolia ETH** (to wrap +
  deposit) on top of its Base-Sepolia gas.

```bash
npm run across:setup      # register the action, deploy + fund the vault (WETH inventory)
npm run across:deposit    # create a real Across intent on Sepolia
npm run across:fill       # relayer fills it via the vault on Base Sepolia
npm run across:attack     # exfiltration is impossible by construction
```

> **Testnet caveat:** Across reimbursement bundles only run on mainnet, so the
> relayer is never repaid for testnet fills — by design, and irrelevant to the
> custody story (which is entirely about the fill-signing path). To productionize
> for mainnet you'd choose a real `repaymentChainId` and add the usual relayer
> inventory/rebalancing, plus an audit of `AcrossSolverVault`.

### Notes from the live run

- **Event schema.** The deployed SpokePools emit the bytes32-addressed
  `FundsDeposited` (uint256 `depositId`) — the SVM-compatible event, newer than
  the address-based `V3FundsDeposited` in Across's published deployment ABIs.
  `acrossPolicy.js` decodes that and narrows the bytes32 fields back to EVM
  addresses for the legacy `fillV3Relay` the vault calls (both the legacy and
  new fill entrypoints are present on the proxy implementation).
- **Exclusive relayer.** Open deposits get filled by whoever's fastest, which on
  a live testnet is usually a public relayer — so the deposit names our vault as
  the exclusive relayer for a window. This is also the realistic solver setup.
- **Latency.** Full authorization round-trip — an `eth_getLogs` on the origin
  chain, three `eth_call`s for the vault's policy config on the destination
  chain, and the threshold sign — measured **~335 ms warm, ~355 ms median**
  (Base/Eth Sepolia + Lit testnet). It was ~0.9–1.1 s before the reads were
  parallelized: `acrossPolicy.js` fires all four reads with `Promise.all`,
  collapsing four sequential RPC round-trips into roughly one, which is the bulk
  of the latency. Cite it as "policy authorization round-trip including on-chain
  reads" — pure threshold signing is a fraction of it, and measure that
  separately if you need the standalone number for the one-pager.
- **RPC consistency.** Alchemy is load-balanced and lags read-after-write; a
  freshly-mined balance/state may be invisible to the next call for a few
  seconds. `deploy-across.js` polls past it; the post-fill balance print can
  still show stale `0.0` momentarily even though the fill succeeded.

## Dashboard

A read-only ops view (live fills, policy state, inventory, kill-switch toggle)
lives in [`dashboard/`](./dashboard) — a small Next.js app pointed at the
`AcrossSolverVault`. Good for a screenshot / Loom frame. See its README to run it.

## Next steps (from the plan)

- **More routes / chains**: the Across path is wired for Sepolia → Base Sepolia;
  add origin chains by allowlisting them on the vault and pointing the action's
  origin RPC + SpokePool at them.
