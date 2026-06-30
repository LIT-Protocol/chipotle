# Agent Context: Backend Services (Rust)

## Purpose
Ops-facing billing service, deployed outside the TEE (Railway). Provides magic-link
auth, the admin credit portal, the LITKEY payment gateway, auto top-up (off-session
card charges when a customer's balance drops below a threshold), enterprise billing,
and a gas funder. Source is organized by domain: `auth/`, `auto_topup/`, `billing/`,
`enterprise/`, `gas_funder/`, `internal/`, `portal/`. See `plans/lit-payments-app.md`
and `plans/auto-top-up.md` for design.

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
