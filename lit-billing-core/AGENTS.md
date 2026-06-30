# Agent Context: Backend Services (Rust)

## Purpose
Shared Stripe primitives — a library crate (no binary, no env vars) used by both
`lit-api-server` (charges, balance checks) and `lit-payments` (credit grants) so the
two services agree on the identity model. Core invariant: every Stripe customer is
keyed by `metadata.wallet_address`; always look up via
`customer::find_or_create_by_wallet`. Modules: `client`, `customer`, `balance`,
`reporting`, `format`, plus `billing_auth/` and EIP-712 helpers.

## Stack & Tooling
- Toolchain: Rust 1.91 (pinned via `rust-toolchain.toml`)
- Crate type: shared library (no binary), consumed by lit-api-server and lit-payments
- Key Libraries: Tokio (async), reqwest (Stripe HTTP client), Rocket 0.5 request guards (`FromRequest`), Alloy (Web3 / EIP-712)
- Linting: `cargo clippy`

## Coding Rules
- Error Handling: Do not use `.unwrap()` or `.expect()` in production paths. Use `Result` and propagate errors with `?`.
- Async: Keep blocking operations outside of the Tokio executor threads using `tokio::task::spawn_blocking`.
- Types: Match Solidity types exactly when decoding events (e.g., `U256` mapping).

## Definition of Done
1. Run `cargo clippy --all-targets -- -D warnings` and fix all warnings.
2. Run `cargo fmt --check`.
3. All tests must pass via `cargo test`.
