# Agent Context: Blockchain

## Purpose
On-chain layer for the API server: the Solidity contracts plus the Rust tooling that
compiles and deploys them. Two subfolders, each with its own `AGENTS.md` — read the
folder-level file before modifying code there.

## Subfolders
- **`lit_node_express/`** — Solidity smart contracts (Diamond / EIP-2535 pattern):
  `AccountConfig` and its facets, shared interfaces and libraries. Built and tested
  with Foundry (`forge`) alongside Hardhat 3; `forge bind` produces the Rust bindings.
  See `lit_node_express/AGENTS.md`.
- **`rust_generator_and_deployer/`** — Rust CLI (`contract_deployer`) that deploys the
  compiled contract artifacts to a chain (Anvil, Yellowstone, Base Sepolia, Base).
  See `rust_generator_and_deployer/AGENTS.md`.

## Language Boundary
Solidity and Rust are kept separate. The contracts are the source of truth; the Rust
side consumes their generated artifacts/bindings. Do not hand-edit generated bindings —
regenerate them from the contracts.
