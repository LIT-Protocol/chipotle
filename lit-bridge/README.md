# lit-bridge

Permissionless cross-chain token bridge — the same product shape as Hyperlane
Warp Routes, but the verification layer is a content-addressed Lit Action that
reads the source chain directly (over N independent RPCs, with quorum) instead
of a validator set. No validators to bootstrap per route or per chain.

See **`plans/hyperlane-competitor.md`** for the full design, trust model, and
phased roadmap. This crate is the productized artifact (it replaces the old
`examples/cross-chain-token` reference).

## Layout

```
lit-bridge/
  src/                Rocket service: bridging UI host + /api/config (stateless)
  static/             web UI (placeholder; Phase 0)
  action/             the bridge verification Lit Action + its unit tests
  contracts/          Foundry: BridgeConfigRegistry (control plane) + BridgeToken
  Dockerfile          repo-root build context; deployed to Railway
  railway.json
```

**Stateless by design** — there is no database. All bridge state lives on-chain:
a pending transfer is a `BurnInitiated` event; a completed one is
`usedBurnIds[burnId] == true` + `BridgeMint` on the destination. The UI reads
that directly. If we later want a fast explorer (cached log scans, analytics),
it's a separate indexer over chain events — a rebuildable cache, not a
dependency of the core service.

## The trust layer (action/)

`action/bridgeAction.js` is the oracle. It:

1. Reads per-chain RPC config from the on-chain **BridgeConfigRegistry** (so
   adding a chain is a governed config write, not a source edit that would
   rotate this action's CID). The only things pinned in code are the registry's
   address + the host allowlist used to read it.
2. Decrypts each RPC secret in-TEE. For `alchemy`/`infura` the hostname is built
   from a code-resident map (config can't redirect those reads); for `custom`
   the registry stores a plaintext host + an encrypted URL and the action
   asserts they agree.
3. Fetches the burn facts from **N independent RPCs and requires M-of-N
   agreement** (quorum; floor 1, default 2). A single lying RPC can't forge a
   mint.
4. Signs the mint authorization with a **dedicated account** (Option B) governed
   by the Base Safe — so action logic can be upgraded without rotating the
   oracle address every BridgeToken trusts.

Run the action's unit tests (pure logic — consensus, provider URLs, registry
host allowlist, finality floor; no secrets needed):

```sh
cd action && node --test
```

## Contracts (Foundry)

```sh
cd contracts
forge build      # auto-installs forge-std; OpenZeppelin via npm (npm i)
forge test
```

- `BridgeConfigRegistry.sol` — owner-governed (Base Safe, two-step) per-chain RPC
  config + quorum. Secrets stored encrypted; hostnames for alchemy/infura never
  stored (constructed in the action).
- `BridgeToken.sol` — burn/mint cross-chain ERC-20 trusting an immutable oracle
  address.

Deploy (needs `DEPLOYER_PRIVATE_KEY`, funded on the target chain):

```sh
# Registry on Base (set REGISTRY_OWNER to the Safe in prod)
forge script script/Deploy.s.sol:DeployRegistry --rpc-url $BASE_SEPOLIA_RPC_URL --broadcast
# A token per chain (ORACLE_ADDRESS = the bridge signing account)
ORACLE_ADDRESS=0x... forge script script/Deploy.s.sol:DeployToken --rpc-url $RPC_URL --broadcast
```

## Service (Rust / Rocket)

```sh
cargo run   # no secrets needed; stateless
```

Endpoints: `GET /health`, `GET /` (UI), `GET /api/config` (registry chain +
address for the UI to bootstrap from).

## Relayer (automation)

In relay mode the action doesn't just sign — it **broadcasts the mint itself**
from the oracle account (fund it with gas via `scripts/fundPkp.js`). A
`lit-triggers` `chain_event` trigger watches `BurnInitiated` on each token and
runs the action, so burns auto-mint on the other side. The relay logic is
proven directly (no triggers instance needed) by:

```sh
cd scripts && node relay.js --amount 10   # burn -> action verifies, signs, AND mints
```

To wire the production trigger (needs a running lit-triggers instance with
`BASE_SEPOLIA_RPC_URL` / `ARBITRUM_SEPOLIA_RPC_URL` set, and an agent token from
its magic-link auth flow):

```sh
TRIGGERS_BASE=... TRIGGERS_AGENT_TOKEN=... node registerTriggers.js
```

(lit-triggers' `CHAIN_SPECS` now include `base-sepolia` / `arbitrum-sepolia`.)

## Upgrades & governance

The signing account is chain-secured and the registry + tokens are owned by the
Base Safe. Shipping a new action or changing config is a **Safe-governed** flow
(re-pin the action CID / propose the owner write, then the Safe owner executes).
Don't wing it — follow **[`docs/upgrading-the-action.md`](docs/upgrading-the-action.md)**
(it has the exact runbook, addresses, and the `GS013` / master-vs-alias-hash
gotcha that bites otherwise). `scripts/proposeRepin.js` automates the propose step.

## Status

Live on testnet, confirmed end-to-end (manual transfer **and** auto-relay):
Base Sepolia ↔ Arb Sepolia. Phases 0–4 done; Phase 5 partial (demo pair live).
Still to do: governance handoff to the Base Safe, hardening, the full
Alchemy ∩ Infura default chain set, a running lit-triggers deployment for the
production relayer, and the real bridging UI.
