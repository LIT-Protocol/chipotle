#!/usr/bin/env bash
# 01-check-prereqs.sh — verify every base requirement is installed correctly and
# usable. This is a test, not an installer: it exits non-zero if anything needed
# to run Chipotle locally is missing or broken.
#
# Checks: git, rustc/cargo/rustup, the pinned 1.91 toolchain actually resolving
# inside a Chipotle crate, anvil/forge/cast, node/npm, jq, static-web-server,
# perl/make/curl, and the dstack simulator binary. Docker is reported but never
# fails the run (optional).
#
# Usage:  bash local-runbook/scripts/01-check-prereqs.sh
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; source "$DIR/_common.sh"
set +e  # this is a test: keep going and tally results, don't abort on first failure

# Make sure tool dirs installed by 00 are visible even in a fresh shell.
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env" || true
export PATH="$HOME/.foundry/bin:$HOME/.local/bin:$PATH"

check_cmd() { # name  [min-hint]
  if command -v "$1" >/dev/null 2>&1; then ok "$1  ($(command -v "$1"))"; else fail "$1 MISSING${2:+ — $2}"; fi
}

hd "Required command-line tools"
check_cmd git
check_cmd rustc "install via rustup"
check_cmd cargo
check_cmd rustup
check_cmd anvil "install Foundry: curl -L https://foundry.paradigm.xyz | bash && foundryup"
check_cmd forge
check_cmd cast
check_cmd node
check_cmd npm
check_cmd protoc "install protobuf: linux 'apt install protobuf-compiler', macOS 'brew install protobuf'"
check_cmd jq
check_cmd static-web-server
check_cmd perl
check_cmd make
check_cmd curl

hd "Versions"
info "rustc:  $(rustc --version 2>/dev/null || echo MISSING)"
info "cargo:  $(cargo --version 2>/dev/null || echo MISSING)"
info "node:   $(node --version 2>/dev/null || echo MISSING)"
info "npm:    $(npm --version 2>/dev/null || echo MISSING)"
info "anvil:  $(anvil --version 2>/dev/null | head -1 || echo MISSING)"
info "forge:  $(forge --version 2>/dev/null | head -1 || echo MISSING)"
info "jq:     $(jq --version 2>/dev/null || echo MISSING)"
info "sws:    $(static-web-server --version 2>/dev/null | head -1 || echo MISSING)"

hd "Rust toolchain 1.91 (pinned by rust-toolchain.toml)"
if rustup toolchain list 2>/dev/null | grep -q '^1\.91'; then
  ok "1.91 toolchain installed"
else
  fail "1.91 toolchain NOT installed — run: rustup toolchain install 1.91"
fi
if CHIPOTLE_DIR="$(resolve_chipotle_dir 2>/dev/null)"; then
  RESOLVED="$( (cd "$CHIPOTLE_DIR/lit-api-server" && rustc --version) 2>/dev/null || echo '?')"
  if echo "$RESOLVED" | grep -q '1\.91'; then
    ok "inside lit-api-server, rustc resolves to: $RESOLVED"
  else
    warn "inside lit-api-server rustc resolves to '$RESOLVED' (expected 1.91). rustup will auto-install on first build."
  fi
else
  warn "Chipotle checkout not found yet — run 02-fetch-chipotle.sh, then re-run this. (Toolchain-in-crate check skipped.)"
fi

hd "dstack simulator"
if [ -f "$SIMULATOR_DIR/dstack-simulator" ]; then
  ok "simulator binary present at $SIMULATOR_DIR/dstack-simulator"
else
  fail "dstack-simulator NOT found at $SIMULATOR_DIR — run 00-install-prereqs.sh (or set SIMULATOR_DIR)."
fi

hd "Optional: Docker (Jaeger tracing only)"
if command -v docker >/dev/null 2>&1; then
  if docker info >/dev/null 2>&1; then info "docker daemon running — Jaeger tracing available";
  else warn "docker installed but daemon not running — Jaeger auto-skipped (fine)"; fi
else
  warn "docker not installed — Jaeger auto-skipped (fine)"
fi

hd "Smoke test: can anvil boot and answer JSON-RPC?"
if command -v anvil >/dev/null 2>&1; then
  anvil --silent --port 8546 >/tmp/anvil-check.log 2>&1 &
  APID=$!
  sleep 2
  if curl -sf http://127.0.0.1:8546 -X POST -H "Content-Type: application/json" \
      -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' >/dev/null 2>&1; then
    ok "anvil answered eth_blockNumber on :8546"
  else
    fail "anvil did not answer JSON-RPC (see /tmp/anvil-check.log)"
  fi
  kill "$APID" 2>/dev/null || true
else
  fail "anvil missing — cannot smoke test the chain"
fi

summary && { hd "All required prerequisites OK"; exit 0; } || { hd "Some prerequisites are missing"; exit 1; }
