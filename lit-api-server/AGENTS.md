# Agent Context: Backend Services (Rust)

## Purpose
The Chipotle REST API — a Rust/Rocket server exposing account, key, group, and Lit
Action execution endpoints under `/core/v1/` (plus `/attestation`, `/health`,
`/dstack/v1/`). Runs inside a TEE (Phala dstack) in production; against the dstack
simulator locally. Source is organized by domain: `accounts/`, `actions/`, `core/`,
`dstack/`, `internal/`, `observability/`.

## Stack & Tooling
- Toolchain: Rust 1.91 (pinned via `rust-toolchain.toml`)
- Key Libraries: Tokio (async), Rocket 0.5 (web), Tonic (gRPC client to lit-actions), Alloy (Web3 interaction)
- Linting: `cargo clippy`

## Coding Rules
- Error Handling: Do not use `.unwrap()` or `.expect()` in production paths. Use `Result` and propagate errors with `?`.
- Async: Keep blocking operations outside of the Tokio executor threads using `tokio::task::spawn_blocking`.
- Types: Match Solidity types exactly when decoding events (e.g., `U256` mapping).

## Definition of Done
1. Run `cargo clippy --all-targets -- -D warnings` and fix all warnings.
2. Run `cargo fmt --check`.
3. All tests must pass via `cargo test`.
