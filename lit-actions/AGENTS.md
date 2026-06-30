# Agent Context: Backend Services (Rust)

## Purpose
Runs user JavaScript (Lit Actions) inside a sandboxed Deno environment. Exposes a
bidirectional gRPC API over a Unix socket (the server side); `lit_node` is the
client. Deno "ops" are proxied back to the client mid-execution. JS extension files
live in `ext/js/`.

## Stack & Tooling
- Toolchain: Stable Rust (latest)
- Key Libraries: Tokio (async), Axum (web), Alloy/Ethers-rs (Web3 interaction)
- Linting: `cargo clippy`

## Coding Rules
- Error Handling: Do not use `.unwrap()` or `.expect()` in production paths. Use `Result` and propagate errors with `?`.
- Async: Keep blocking operations outside of the Tokio executive thread using `tokio::task::spawn_blocking`.
- Types: Match Solidity types exactly when decoding events (e.g., `U256` mapping).

## Definition of Done
1. Run `cargo clippy --all-targets -- -D warnings` and fix all warnings.
2. Run `cargo fmt --check`.
3. All tests must pass via `cargo test`.
