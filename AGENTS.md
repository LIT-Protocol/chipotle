# Repository Agent Context (Root)

## Purpose
Monorepo root for Chipotle — the Lit Protocol management API and its surrounding
services. This file holds repo-wide conventions; each subfolder has its own
`AGENTS.md` describing what lives there. Start here, then read the folder-level file
before modifying code.

## Project Overview
This is a Web3/crypto monorepo. It features a Rust backend, Solidity smart contracts,
and a JavaScript/TypeScript frontend / monorepo tooling layer.

## Monorepo Architecture
- Tooling: Rust crates built per-crate with Cargo (no repo-root workspace); the `e2e`
  suite uses pnpm.
- Language Boundaries:
  - `lit-actions`, `lit-api-server`, `lit-billing-core`, `lit-payments`,
    `lit-triggers`, `lit-core`: Rust (services & indexers)
  - `lit-static`: JavaScript (static dapps, SDK bundles, contract ABIs)
  - `e2e`: TypeScript / Playwright (end-to-end tests) — see `e2e/AGENTS.md`
  - `otel-collector`: OpenTelemetry collector config

## Global Constraints
- CRITICAL: Never mix package commands. Use each language's own tool — `cargo`
  (per-crate `--manifest-path`) for Rust, `pnpm` for the `e2e` suite. There is no
  repo-root cargo workspace, so build each crate from its own `Cargo.toml`.
- Always check for a folder-level `AGENTS.md` inside subdirectories before modifying
  code there.
- Language boundaries are strict. Do not introduce Rust dependencies into JS tools, or
  vice versa, without human approval.
