# Sample Hardhat 3 Beta Project (minimal)

This project has a minimal setup of Hardhat 3 Beta, without any plugins.

## What's included?

The project includes native support for TypeScript, Hardhat scripts, tasks, and support for Solidity compilation and tests.

## Verifying contracts on Basescan

The diamond proxies on Base are long-lived and their facets were compiled from several
different commits, so verification runs from Hardhat `build-info` (the exact standard-JSON
input that produced each artifact) rather than from a fresh compile of one tree.

The `verify-basescan` task reads the live facet list via the loupe, matches each address to
a compiled artifact by runtime bytecode (exact metadata match preferred), and submits the
matching build-info to the Etherscan v2 API for chain 8453. It skips addresses Basescan
already reports as verified. The proxy's constructor args are recovered from its creation
transaction (found via Etherscan, then Blockscout, then an RPC bisection of when code first
appeared), so no manual ABI-encoding is needed; `--constructor-args` overrides if needed.
A free Etherscan key works (the task throttles to the 3 req/s free-tier limit).

```bash
# 1. Compile every source state the diamond's code came from (one checkout each):
git worktree add /tmp/lne-rel   origin/release/v1.1.10   # app facets currently on prod
git worktree add /tmp/lne-proxy c654d704                 # AccountConfig proxy (Mar 2026 deploy)
git worktree add /tmp/lne-dp    91593f09                 # contracts/DiamondPattern/* (prebuilt facets)
for d in /tmp/lne-rel /tmp/lne-proxy /tmp/lne-dp; do
  (cd $d/lit-api-server/blockchain/lit_node_express && npm ci && npx hardhat compile)
done

# 2. Dry-run to see the match plan (no API key needed):
npx hardhat verify-basescan --network base --dry-run \
  --diamond 0xaaaaa9120fe271f653cfdb6bf400db93d2dea7aa \
  --artifacts /tmp/lne-rel/lit-api-server/blockchain/lit_node_express/artifacts,/tmp/lne-proxy/lit-api-server/blockchain/lit_node_express/artifacts,/tmp/lne-dp/lit-api-server/blockchain/lit_node_express/artifacts

# 3. Submit:
BASESCAN_API_KEY=... npx hardhat verify-basescan --network base --diamond 0x... --artifacts ...
```

Notes:
- Hardhat compiles with `evmVersion: paris`; `forge build` defaults to `cancun` and produces
  different bytecode, so always compare against Hardhat artifacts.
- The three diamond-pattern facets (`DiamondCutFacet`, `DiamondLoupeFacet`, `OwnershipFacet`)
  are deployed from the prebuilt JSONs in `rust_generator_and_deployer/src/diamond/`, which were
  compiled when those sources lived at `contracts/DiamondPattern/` (commit `91593f09`). The
  same sources now live in `libraries/diamond/` but compile to a different metadata hash there.
- After each future `diamondCut`, rerun the task with the release checkout that was deployed
  so the new facet addresses get verified too.

### Source commits per live address (Base mainnet, as of 2026-09-10)

All 30 addresses below were verified on Basescan on 2026-09-10.

Every address below reproduces its on-chain runtime bytecode exactly (metadata included)
when compiled with Hardhat at the listed commit.

| Diamond | Address | Contract(s) | Compile at |
|---|---|---|---|
| all | proxy + DiamondCut/Loupe/Ownership facets | `AccountConfig`, `contracts/DiamondPattern/*` | `91593f09` (prod proxy also matches `c654d704`) |
| prod `0xaaaaa912…` | `0xC6912c58…`, `0x5698f83d…`, `0x7c251374…`, `0x2Aa0f31E…` | APIConfig, Billing, Views, Writes | `origin/release/v1.1.10` (`ae36a42c`) |
| main `0x4c8eb9f3…` | `0xB9505606…`, `0xa72b34C6…`, `0xe30E45B3…`, `0xb6Ad578C…` | APIConfig, Billing, Views, Writes | `07f36409` |
| main | `0x7122e21c…`, `0xccf68FEb…` | stale Views/Writes (dead selectors) | `065943f7` |
| main | `0x5130106F…` | stale Writes (dead selectors) | `91593f09` |
| next `0x98e501fa…` | `0x425a0988…`, `0x017A1607…`, `0x3dE91525…` | APIConfig, Billing, Writes | `b3d8d0ce` |
| next | `0xb17128B8…` | Views | `a4284418` |
| next | `0xAd43d683…`, `0x6CE23AE1…` | stale Views/Writes (dead selectors) | `17a635f5` |
| next | `0x1525dE5F…` | stale Writes (dead selectors) | `91593f09` |

The "stale" rows are old facets that still own a handful of selectors that later versions
dropped without a `Remove` cut. They are harmless but should be cleaned up in a future cut.
