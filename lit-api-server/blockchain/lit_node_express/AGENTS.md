# Agent Context: Smart Contracts (Solidity)

## Purpose
The on-chain contracts for Chipotle, built on the Diamond (EIP-2535) pattern.
`AccountConfig.sol` is the diamond proxy; its facets live in `AccountConfigFacets/`,
with shared `interfaces/` (IDiamond, IDiamondCut, IDiamondLoupe, IERC165/173) and
`libraries/` (`LibDiamond.sol`, diamond helpers). Tests are in `test/`, Hardhat
scripts/tasks in `tasks/`. `forge bind` generates the Rust bindings consumed by the
deployer; `generate-diamond-abi.mjs` / `postprocess-forge-bindings.mjs` assemble the
combined diamond ABI.

## Stack & Tooling
- Compiler: Solidity v0.8.28 (pinned in `foundry.toml`, `auto_detect_solc = false`),
  `via_ir = true`, optimizer on (100 runs).
- Framework: Foundry (`forge`) for build/test/bindings, alongside Hardhat 3 (`hardhat.config.ts`).
- Linters: Solhint, Slither. Formatting via `forge fmt` (config in `foundry.toml` `[fmt]`).

## Strict Coding Rules
- Security First: Always protect against reentrancy. Use `ReentrancyGuard` or the Checks-Effects-Interactions pattern.
- Diamond discipline: never collide storage slots across facets — use diamond storage (namespaced structs) and keep selectors unique across facets.
- Gas Optimization: Prefer custom errors over `require` string messages. Use `uint256` unless tightly packing structs.
- Code Style: Follow standard NatSpec formatting for all public/external functions.

## Definition of Done
1. Compile successfully with `forge build`.
2. Write unit tests for all public state-changing functions.
3. Ensure `forge test` passes with zero failures.
4. Regenerate bindings (`forge bind` + the postprocess scripts) when the ABI changes, so the Rust deployer stays in sync.
