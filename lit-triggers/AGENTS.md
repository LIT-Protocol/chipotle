# Agent Context: Backend Services (Rust)

## Purpose
Reactive Lit Action runner service. Users sign in with magic links, create trigger
configs, and store scoped Chipotle usage API keys encrypted at rest. Supports
webhook, scheduled, and EVM chain-event triggers that enqueue runs for a shared
dispatcher. Backed by Postgres. Key modules: `auth/`, `chain_events.rs`,
`dispatcher.rs`, `chipotle.rs`, `crypto.rs`, `mail.rs`.

## Stack & Tooling
- Toolchain: Rust 1.91 (required by alloy)
- Key Libraries: Tokio (async), Rocket 0.5 (web), alloy-primitives (Web3 types), Postgres
- Linting: `cargo clippy`

## Coding Rules
- Error Handling: Do not use `.unwrap()` or `.expect()` in production paths. Use `Result` and propagate errors with `?`.
- Async: Keep blocking operations outside of the Tokio executor threads using `tokio::task::spawn_blocking`.
- Types: Match Solidity types exactly when decoding events (e.g., `U256` mapping).

## Definition of Done
1. Run `cargo clippy --all-targets -- -D warnings` and fix all warnings.
2. Run `cargo fmt --check`.
3. All tests must pass via `cargo test`.
