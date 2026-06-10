# Bringing a token to new chains with lit-bridge

lit-bridge makes a token live on multiple chains with a **burn/mint** model: burn
on chain A, an oracle verifies the burn and authorizes a mint on chain B. The
oracle is a **content-addressed Lit Action** that reads the source chain over N
independent RPCs and requires M-of-N agreement — there is **no validator set to
bootstrap**, which is what makes adding a chain cheap and permissionless.

This guide is for token issuers. Read it before you deploy anything.

## What's supported today (and what isn't)

- ✅ **A new burn/mint token across EVM chains.** `BridgeToken` is an ERC-20 that
  is the same contract on every chain; supply moves by burn→mint. This is what
  `scripts/setup.js` deploys.
- ✅ **Adding chains** that the action can reach (see "Adding a new chain").
- ⛔ **Wrapping an EXISTING token** (lock-and-mint on its home chain, synthetic
  elsewhere) is **not built yet** — it needs the collateral/synthetic *router*
  contracts (tracked in `plans/hyperlane-competitor.md`). If you have a live
  token already, that's the path you want; it's the next contract work.
- ⛔ **Non-EVM chains** are out of scope.

## Two ways to run it

### A. Your own instance (permissionless, recommended)

You deploy and govern everything — your own oracle account, registry, tokens, and
relayer. No dependency on anyone else. This is the permissionless path and the
one the scripts are built for.

You will own:
- a **Lit account** (the signing oracle — start API-key, then chain-secure it to
  your Safe; see `docs/upgrading-the-action.md` for the Safe model),
- a **BridgeConfigRegistry** (per-chain RPC config + quorum),
- a **BridgeToken** on each chain,
- the **relayer** (lit-triggers chain-event triggers + `retryPoller.js`).

### B. Use a shared deployment

If an operator already runs a lit-bridge oracle + registry for your chains, you
can deploy `BridgeToken`s pinned to *their* oracle address and ask them to add
your tokens to their relayer. Lighter, but you depend on that operator for
relaying and for adding new chains to their registry. Most issuers should prefer
A.

## Launching your own (path A)

Prerequisites:
- A master **Lit API key** (`dashboard.chipotle.litprotocol.com`).
- A **deployer EOA** funded on every target chain.
- **Alchemy + Infura API keys** (two independent providers → quorum 2). Each
  default chain ships with both; the keys are encrypted and stored in the
  registry, never exposed.

Steps:
1. `cp lit-bridge/.env.example lit-bridge/.env` and fill in the required keys.
   Set `FEE_TREASURY`, `FEE_BPS` (default 0.1%), `CHAIN_QUORUM` (default 2),
   `INITIAL_SUPPLY` / `INITIAL_SUPPLY_NETWORK` (the home chain that gets supply).
2. `cd lit-bridge/scripts && npm install`.
3. `node setup.js` — deploys the registry (on Base), creates the oracle account +
   PKP, encrypts your RPC keys against it, writes per-chain config, deploys a
   `BridgeToken` on each chain, wires `bridgePartner` both ways, and sets the fee.
   It's resumable (writes results to `.env`); re-run if interrupted. (Don't pipe
   it through `head` — SIGPIPE kills it mid-run.)
4. **Fund the oracle for gas:** `node fundPkp.js 0.01` (the relayer pays
   destination-mint gas from the oracle account).
5. **Stand up the relayer:** authorize an agent with lit-triggers, then
   `node registerTriggers.js` (one `chain_event` trigger per chain watching
   `BurnInitiated`). Run `node retryPoller.js` continuously (cron `--once` every
   ~2 min, or a managed process) to recover any burns the single-fire trigger
   misses.
6. **Test:** `node burn.js --amount 5` then `node watchRuns.js <baseTriggerId>`.

Each holder now bridges with `BridgeToken.burn(amount, destChainId, recipient)`
(attaching a small native gas prepay so the relayer is reimbursed). The relayer
auto-mints on the destination; without a prepay the holder can self-submit
`mint` with the oracle signature and pay their own gas.

## Adding a new chain

A chain needs two things in the action:

1. **An RPC the action can build.** For chains in the action's `ALCHEMY_SUBDOMAINS`
   / `INFURA_NETWORKS` maps (Ethereum, Base, Arbitrum, Optimism, Polygon, +
   testnets), you just supply the API key. For any other chain, use a **`custom`**
   registry entry: store the plaintext host + the encrypted full RPC URL; the
   action verifies the decrypted URL's host matches. Adding a chain to the
   alchemy/infura maps is a code change (→ new action CID → re-pin via the Safe;
   see `docs/upgrading-the-action.md`); a `custom` entry is pure config.
2. **A registry entry** (`setChain(chainId, minConfirmations, quorum, rpcs)`) and a
   `BridgeToken` deployed there, wired to its siblings via `bridgePartner`. With a
   chain-secured account these are Safe-governed writes.

Notes:
- **Quorum ≥ 2 with independent providers.** Quorum 1 is an escape hatch for a
  chain only one provider covers, but it drops to single-RPC trust — avoid it for
  anything with value.
- **Same native asset for auto-relay.** The relayer's gas-prepay check compares
  source-prepaid wei to destination-gas wei 1:1, so auto-relay is only enabled
  between chains with the same native token (all ETH-native L2s today). A
  cross-native pair (e.g. an ETH chain ↔ Polygon) needs an exchange-rate step —
  not built yet; such pairs can still bridge via manual submission.
- **Confirmations / finality.** `minConfirmations` is per-chain and
  Safe-governed; set it conservatively for chains with deeper reorg risk. The
  retry poller keeps re-attempting until a burn is final and minted.

## The trust model in one paragraph

A mint is honored only if: the published action code (its CID) is what runs; the
Lit node network executes it honestly in-TEE; the Safe-governed registry config
wasn't maliciously changed; M-of-N independent RPCs agree on the burn; and TLS to
those RPCs holds. You are trading a validator set for "audited action + node
network + your Safe + RPC consensus." Adding a chain is a config write, not a new
validator quorum to bootstrap — that's the whole point.
