#!/usr/bin/env bash
# 03-warm-build.sh — pre-download and compile everything so that the LOCAL RUN
# needs no internet. NEEDS THE INTERNET (this is the last install-phase step).
#
# What it warms:
#   1. Contract deps + Solidity compile (npm ci + hardhat compile) — caches solc
#      and node_modules so deploy_anvil runs offline afterward.
#   2. contract_deployer (Rust) — the binary local_test.sh invokes to deploy.
#   3. lit-api-server and lit_actions (Rust) — the two long compiles; downloads
#      all crates from crates.io now so the run is offline.
#
# After this succeeds once WITH internet, `04-run-local.sh` can run fully offline.
#
# Usage:  CHIPOTLE_DIR=/path/to/chipotle bash local-runbook/scripts/03-warm-build.sh
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; source "$DIR/_common.sh"

[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env" || true
export PATH="$HOME/.foundry/bin:$HOME/.local/bin:$PATH"

CHIPOTLE_DIR="$(resolve_chipotle_dir)" || die "Chipotle checkout not found. Run 02-fetch-chipotle.sh or set CHIPOTLE_DIR."
hd "Warming build at $CHIPOTLE_DIR"

CONTRACTS_DIR="$CHIPOTLE_DIR/lit-api-server/blockchain/lit_node_express"

hd "1/3  Contracts: npm ci + hardhat compile"
# npm ci is deterministic (installs exactly package-lock.json) — better for a
# reproducible, offline-ready cache. Falls back to npm i if no lockfile is present.
( cd "$CONTRACTS_DIR" && { [ -f package-lock.json ] && npm ci || npm i; } && npx hardhat clean && npx hardhat compile )
ok "contracts compiled (solc + node_modules cached)"

hd "2/3  contract_deployer (Rust)"
( cd "$CHIPOTLE_DIR/lit-api-server/blockchain/rust_generator_and_deployer" && cargo build --bin contract_deployer )
ok "contract_deployer built"

hd "3/3  lit-api-server + lit_actions (Rust — this is the long one)"
info "Building lit-api-server..."
( cd "$CHIPOTLE_DIR/lit-api-server" && cargo build --bin lit-api-server )
ok "lit-api-server built"
info "Building lit_actions..."
( cd "$CHIPOTLE_DIR/lit-actions" && cargo build --bin lit_actions )
ok "lit_actions built"

hd "Warm build complete"
info "All crates and node deps are cached. You can now run offline:"
info "   CHIPOTLE_DIR=$CHIPOTLE_DIR bash local-runbook/scripts/04-run-local.sh"
