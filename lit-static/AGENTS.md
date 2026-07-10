# Agent Context: Static Assets (JavaScript)

This folder holds **browser-served static assets** — dapps, SDK bundles, and ABIs.
It is NOT a Rust crate; the backend Rust rules do not apply here.

## Purpose
Browser-facing static assets served by `static-web-server` (locally on `:8080` via
`local_test.sh`; behind the dashboard domain in production). Contains the JS Core SDK
(`core_sdk.js` — the client for the `/core/v1/` API), the Chipotle dapps under
`dapps/` (`dashboard/`, `monitor/`, `verify/`), AccountConfig contract ABIs
(`*_abi.js`), and WalletConnect / tx-lifecycle helpers. No build step — files are
served as-is.

## Stack & Tooling
- Plain JavaScript + HTML served as static files (no build step, no bundler).
- Contents: `dapps/`, `core_sdk.js`, `wallet_connect.js`, `*_abi.js`, `tx_lifecycle.js`.

## Coding Rules
- Keep files framework-free and dependency-free unless a human approves otherwise — these ship as-is to the browser.
- ABI files (`*_abi.js`) must match the deployed Solidity contracts exactly. Do not hand-edit ABIs; regenerate them from source.
- Match Web3 types carefully (e.g., `BigInt`/`U256` boundaries) when talking to contracts.

## Definition of Done
1. Files load and run in the browser without console errors.
2. ABIs match the on-chain contracts they target.
3. No new external dependencies introduced without human approval.
