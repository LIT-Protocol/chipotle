# Agent Context: Contract Deployer (Rust)

## Purpose
Rust CLI (`lit-contracts-minimal-generator`, binary `contract_deployer`) that deploys
the compiled Lit blockchain contract artifacts to a chain — Anvil, Yellowstone, Base
Sepolia, or Base. Reads Hardhat/Foundry-style artifact JSON (`abi` + `bytecode`) from
the `lit_node_express` output and uses a built-in dev wallet (Anvil account #0) unless
`--secret` is passed. Source: `bin/` (entrypoint), `deployer/` (deploy logic),
`diamond/` (EIP-2535 cut/loupe helpers), `args.rs` (CLI args). Local/testnet use only.

## Stack & Tooling
- Toolchain: Rust `1.91` (pinned in `rust-toolchain.toml`), edition 2024.
- Key Libraries: Tokio (async), Alloy 1.0 (`contract`, `dyn-abi`, `json-abi`, `network`, `providers`, `rpc-types`) for Web3 interaction.
- Linting: `cargo clippy`.

## Coding Rules
- Error Handling: Do not use `.unwrap()` or `.expect()` in production paths. Use `Result` and propagate errors with `?`.
- Async: Keep blocking operations outside of the Tokio executive thread using `tokio::task::spawn_blocking`.
- Types: Match Solidity types exactly when encoding calldata / decoding events (e.g., `U256` mapping); these must track the `lit_node_express` contracts.
- Never commit private keys; the dev wallet is for local/testnet only.

## Definition of Done
1. Run `cargo clippy --all-targets -- -D warnings` and fix all warnings.
2. Run `cargo fmt --check`.
3. All tests must pass via `cargo test`.
4. Build the release binary with `cargo build --release` (produces `target/release/contract_deployer`).
