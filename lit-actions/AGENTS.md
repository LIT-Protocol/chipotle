# Agent Context: Backend Services (Rust)

## Purpose
Runs user JavaScript (Lit Actions) inside a sandboxed Deno environment. Exposes a
bidirectional gRPC API over a Unix socket (the server side); `lit_node` is the
client. Deno "ops" are proxied back to the client mid-execution. JS extension files
live in `ext/js/`.

## Stack & Tooling
- Toolchain: Rust 1.91 (pinned via `rust-toolchain.toml`)
- Architecture: multi-crate Cargo workspace (`[workspace]` in `lit-actions/Cargo.toml`)
- Key Libraries: Tokio (async), Tonic (gRPC server over a Unix socket), Deno core (sandboxed JS execution)
- Linting: `cargo clippy`

## Coding Rules
- Error Handling: Do not use `.unwrap()` or `.expect()` in production paths. Use `Result` and propagate errors with `?`.
- Async: Keep blocking operations outside of the Tokio executor threads using `tokio::task::spawn_blocking`.
- Types: Match Solidity types exactly when decoding events (e.g., `U256` mapping).

## Definition of Done
1. Run `cargo clippy --all-targets -- -D warnings` and fix all warnings.
2. Run `cargo fmt --check`.
3. All tests must pass via `cargo test`.
