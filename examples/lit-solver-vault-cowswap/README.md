# Lit Solver Vault — CoW Protocol

**Policy-gated key custody for a CoW Protocol solver. The key that settles
batches lives in a Lit Action, not on the solver's box — so a compromised bot
can't push a self-dealing settlement, and operational guardrails are enforced at
signing time.**

**Fast enough to solve with: ~361 ms median warm** for a full policy
authorization round-trip (vault config reads + EIP-712 order verification +
Lit Action signing), measured live on Base Sepolia + Lit Chipotle. This is the
latency added before the solver submits `settle()`.

This is the CoW sibling of [`lit-solver-vault`](../lit-solver-vault) (which ships
a generic mock demo and a live **Across** relayer). Same custody story, a very
different settlement system — and the difference is the whole point.

> **Runs live on testnet — but not the way Across does.** Across fills are
> *permissionless*: anyone can call `fillV3Relay`, so that example just shows up
> on testnet and fills a real deposit. CoW settlement is *permissioned*: only an
> allowlisted (and, on mainnet, bonded + KYC'd) solver may call `settle()`. You
> can't run a self-serve test solve against CoW's canonical contracts without
> onboarding with the CoW team. So this example deploys *its own*
> `GPv2Settlement` + `GPv2AllowListAuthentication` on Base Sepolia — the
> real, audited CoW contracts, from the published `@cowprotocol/contracts`
> artifacts. Because we deploy our own settlement, **any EVM chain works**; we
> use Base Sepolia for its fast (~2s) blocks. It allowlists the vault as the
> solver, and runs a genuine `settle()` end-to-end
> against real EIP-712 order signatures. Just on an instance we control.

## Why this is a Lit-shaped problem (and why it's sharper for CoW)

A CoW solver runs a bot that holds the **settlement key** — the address that's on
the protocol's solver allowlist and signs the `settle()` transaction. Compromise
the box and you have that key. CoW settlement is *permissioned* but
*unconstrained*: the allowlisted solver can submit **any** batch the protocol
accepts, and a settlement is an arbitrary multicall (`interactions`). A
compromised solver key can craft a batch whose interactions route the solver's
own inventory — or the settlement contract's buffers — to an attacker.

So "permissioned" does **not** mean "safe if the key leaks." It's the same threat
the Across variant removes, with a bigger blast radius. Here:

- The vault contract **is** the allowlisted solver — the bot is not. The bot
  can't call `settle()` at all (it isn't on the allowlist); it can only *ask* the
  vault, and the vault only settles batches a Lit Action signed.
- The Lit Action **builds the entire settlement** from the trader's signed order
  and signs only that. There's no caller-supplied field — recipient, amounts,
  clearing prices and interactions are all derived from the order — so a
  compromised bot can't produce a self-dealing batch. Exfiltration isn't
  "rejected," it's impossible by construction (same property as `acrossPolicy`).
- The vault's inventory is exposed to the settlement only through a **bounded,
  per-batch approval** that's reset to zero after the call — so even the
  settlement contract can never pull more than the one batch needs.

The signature comes from `Lit.Actions.getLitActionPrivateKey()` — a key derived
deterministically from the action's IPFS CID. The deployed `CowSolverVault` pins
that address as its `policySigner`. **Edit the policy by a byte and the CID, key,
and address all change — so the vault stops honoring the modified policy
automatically.**

## How it works

```
   solver bot         Lit Action          Base Sepolia (our own CoW stack)
   (usage key)        (cowPolicy)        CowSolverVault + GPv2Settlement
       │                  │                          │
       │ order + js_params│                          │
       ├─────────────────►│                          │
       │                  │ read rpc host ✓           │
       │                  │ read vault.settlement(),  │
       │                  │ killSwitch, maxFillAmount ├─────────►│
       │                  │◄──────────────────────────┤
       │                  │ read settlement.domainSeparator()    │
       │                  │ verify trader EIP-712 order sig       │
       │                  │ build the WHOLE settle() batch:       │
       │                  │   tokens, clearing prices, the trade, │
       │                  │   + 2 inventory interactions          │
       │ sig + calldata   │ sign(keccak(calldata), pullToken,     │
       │◄─────────────────┤ pullAmount, authDeadline, vault, cid) │
       │                                                          │
       │ executeSettlement(calldata, pullToken, pullAmount, deadline, sig) ──►│
       │                                       recover(sig) == policySigner ✓ │
       │                                       approve pullAmount, then        │
       │                                       GPv2Settlement.settle(calldata) │
       │                                         pulls sellToken from trader   │
       │                                         pays buyToken to receiver     │
       │                                         (from the vault's inventory)  │
```

The batch is a single-order settlement: the trader sells token A for token B; the
vault provides B from inventory and keeps the A it bought. The action fills the
order exactly at its limit (clearing prices `[buyAmount, sellAmount]`), pulls
exactly `buyAmount` of B from the vault via a pre-interaction, and returns the
`sellAmount` of A the settlement collected to the vault via a post-interaction.

## Trust model

Five roles, deliberately separated:

| Role | Holds | Can do | Can't do |
| --- | --- | --- | --- |
| **trader** | their own key | sign an order; approve the VaultRelayer | nothing about the vault |
| **solver bot** | a scoped Lit usage key | *request* a settlement authorization | settle; move inventory; change policy; exit |
| **policySigner** | CID-derived key, inside Lit | build + authorize a valid settlement | be edited without changing its own address |
| **owner** | a key, **recommend a Safe** | change policy (cap, kill switch); `exit` to cold wallet | redirect a settlement; change the exit destination without the timelock |
| **coldWallet** | — | receive emergency exits | — |

The headline property: **a compromised solver bot** (attacker has the usage key
*and* the box) can ask for settlements, but the action only ever signs a
settlement reconstructed from a real, trader-signed order paying that order's real
receiver. And even with the usage key, the bot can't call `settle()` directly —
it isn't an allowlisted solver, only the vault is. To actually steal you'd need to
compromise the `owner` key too — and even then you can only sweep to the pinned
`coldWallet` (changing that destination is timelocked).

**`owner` is fully trusted** — treat it like a treasury key and use a Safe. The
config setters let it *expand* policy (raise the cap, disable the kill switch), so
a compromised owner can widen the gate. The security story is about removing the
*hot settlement key from the bot*, not about constraining the owner.

## Files

| Path | Purpose |
| --- | --- |
| `action/cowPolicy.js` | The policy Lit Action. Reads the vault's pinned settlement + config, verifies the trader's EIP-712 order, **builds the entire `settle()` calldata** from it, enforces cap / kill switch, and signs the batch. |
| `contracts/CowSolverVault.sol` | Inventory custody + the allowlisted solver. `executeSettlement` verifies a policy signature over the settle calldata, grants a bounded one-batch approval, forwards `settle`; `exit` sweeps to the cold wallet; cold-wallet changes are timelocked. |
| `contracts/MockERC20.sol` | Faucet tokens (a 6-dp "USDC" the trader sells, an 18-dp "WETH" the vault holds). |
| `scripts/deploy-cow.js` | Deploys our own `GPv2AllowListAuthentication` + `GPv2Settlement` (real CoW artifacts), the tokens, and the vault; allowlists the vault; funds inventory. |
| `scripts/setup-cow.js` | One-shot: compute the action CID, derive `policySigner`, create + wire the group/usage key, register the action, run the deploy. |
| `scripts/order.js` | Plays the trader: mints the sell token, approves the VaultRelayer, signs an EIP-712 order (the intent). |
| `scripts/solve.js` | Happy path: authorize via Lit, submit `executeSettlement`. |
| `scripts/attack.js` | Three attacks, all defeated: tampered receiver, forged policy sig, and a non-solver trying to settle directly. |
| `scripts/_cow.js` / `_chipotle.js` / `_env.js` | Shared helpers (CoW ABIs + order signing, Lit REST calls, `.env` upsert). |

## Walkthrough

### 1. Fill in your inputs

```bash
cp .env.example .env
npm install
```

Set `LIT_API_KEY`, `ALCHEMY_BASE_SEPOLIA_URL` (a `base-sepolia.g.alchemy.com`
URL — the action whitelists that host), `DEPLOYER_PRIVATE_KEY` (Base-Sepolia gas;
it becomes the vault owner), and `SOLVER_PRIVATE_KEY` (the bot's gas account; can
be the same key for testing). See the comments in `.env.example`.

### 2. Run setup

```bash
npm run setup
```

Six steps, printed as they go: compute the action's CID, create a permission
group (wildcard allowlist, bounded by the scoped usage key), mint a scoped usage
key, derive the action's wallet (the vault's `policySigner`), register the action
+ add it to the group, then deploy the whole CoW stack (`scripts/deploy-cow.js`):
our `GPv2AllowListAuthentication`, our `GPv2Settlement`, the two test tokens, the
`CowSolverVault` (allowlisted as the solver, funded with mWETH inventory).

Re-running does a fresh setup top-to-bottom and orphans the previous
group/key/contracts — the simplest reset for a docs example.

### 3. Walk the demo

```bash
# Cold start — there is no settlement key in the bot's environment:
grep -ri "PRIVATE_KEY" action/     # nothing — the action signs with a CID-derived key

# The trader signs an order and approves the VaultRelayer (the intent):
npm run order

# Happy path: Lit builds + authorizes the settlement, the vault settles it.
npm run solve

# Attacks — all defeated:
npm run attack
#   1. compromised bot rewrites the order receiver to itself -> policy refuses
#      (the rebuilt batch no longer recovers to the order owner)
#   2. bot forges a policy signature and calls executeSettlement -> reverts
#      InvalidPolicySignature
#   3. bot calls GPv2Settlement.settle directly -> "GPv2: not a solver"
#      (only the vault is allowlisted)
```

`npm run solve` prints the policy authorization latency in milliseconds. The
current live measurement on Base Sepolia + Lit Chipotle was **~361 ms median warm**
for authorization-only samples (`538, 484, 322, 360, 361, 352 ms`; mean
`~403 ms`). The first end-to-end solve in this run authorized in `1585 ms`, then
warm solves authorized in `553 ms` and `502 ms`, so expect the first call after
an action edit or cache miss to be slower while the CID is pinned/distributed.

The action does its work in a **single RPC round-trip**: the three vault reads
fire concurrently (`Promise.all`), the EIP-712 domain is computed locally rather
than read (verified byte-for-byte against the deployed settlement), and the
policy-key derivation is kicked off in parallel with the reads. The action only
reads + signs — it does **not** wait for the settlement to mine; `solve.js`
submits that tx after the authorization returns, so block inclusion / finality is
chain latency on top of the Lit authorization.

## Liveness & exit

Same promise as the rest of the family — *"Lit guards your operations; your Safe
guards your inventory — Lit can never block you from your money."*

- **Lit outage = stop settling, not stuck capital.** No settlements can be
  authorized, but `exit(token)` works any time and needs no Lit (and no CoW).
- **The exit destination is pinned.** Even a compromised `owner` key can only push
  funds to `coldWallet`; changing it is `requestColdWalletChange` then
  `commitColdWalletChange` after a 7-day timelock.
- **Time-sensitive policy is re-enforced on-chain.** `executeSettlement` re-checks
  the kill switch and the per-batch cap and bounds the authorization TTL
  (`MAX_AUTH_TTL`), so a signature minted while policy was permissive can't
  execute after the owner tightens it.

## Production hardening

- **The policy here is a single-order solution builder.** It authoritatively
  *constructs* the batch (and its interactions) from one trader-signed order, so
  there's nothing for the bot to tamper with. A real solver settles **multi-order
  batches with bot-proposed interactions** (AMM swaps, internalized trades,
  buffers). There the policy can't just rebuild the batch — it must **validate a
  bot-proposed settlement**: bind each trade's `receiver` to its order (fetched +
  signature-checked), check clearing prices against each order's limit, and
  **allowlist every interaction target/selector** so a malicious interaction can't
  route inventory out. That interaction allowlist is the load-bearing, genuinely
  hard part, and it has no analog in the Across variant (a fill is one bound
  transfer). It's the main thing to build before this is more than a custody demo.
- **Economic policy is minimal.** With arbitrary test tokens the action can't
  price the trade, so it only checks the per-batch cap and exact-limit fill. A
  production solver needs a real fee floor, a quote/orderbook check, an
  output-token allowlist, and a **cumulative** exposure budget — the per-batch cap
  alone doesn't bound total drain across many settlements. (`order.feeAmount` is
  required to be 0 here so the batch stays exact; real orders may carry a fee.)
- **Move policy config off-chain.** `killSwitch` / `maxFillAmount` live on the
  vault so updates are one tx and the action reads them with a plain `eth_call`.
  That's public — it leaks your caps to competitors. In production keep config in
  a signed off-chain blob the action verifies, or behind a private endpoint.
- **Make `owner` and `coldWallet` Safes.** The demo uses the deployer EOA.
- **Audit.** `CowSolverVault` holds real inventory and forwards arbitrary
  settlement calldata to the settlement contract; it needs an audit before any
  mainnet deployment. It is unaudited here.

## Notes

- **Validated end-to-end locally.** The full path — deploy the real GPv2 stack,
  sign an order, build the batch in the policy's exact shape, and run
  `executeSettlement` → `settle()` — was run against the real `@cowprotocol/contracts`
  on an in-memory chain: the receiver is paid from inventory, the vault keeps the
  bought token, and both attack paths revert. The on-chain `settle()` re-verifies
  the order signature, so the policy's checks are belt-and-suspenders, not the
  only line of defense.
- **Latency from the live run.** On June 8, 2026, the policy authorization path
  measured `~361 ms` median warm against Base Sepolia + Lit Chipotle. This is the
  extra solver-side wait before submitting `executeSettlement`; the subsequent
  `settle()` transaction still waits for normal chain inclusion.
- **erc20 balances only.** The action builds sell / fill-or-kill / erc20 orders,
  so the Balancer Vault that `GPv2Settlement` references is never called (payout
  is a direct `safeTransfer`). That's why a self-deployed settlement works without
  standing up Balancer.
- **RPC consistency.** Alchemy is load-balanced and can lag read-after-write; a
  freshly-mined balance may be invisible to the next call for a few seconds.
