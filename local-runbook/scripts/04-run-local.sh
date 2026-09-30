#!/usr/bin/env bash
# 04-run-local.sh — boot the full Chipotle stack locally, payment-free, against a
# throwaway local chain. NO STRIPE, NO LIVE CHAIN.
#
# It sets LIT_DISABLE_BILLING=true (so lit-api-server makes ZERO Stripe calls)
# and runs the repo's local_test.sh, which starts Anvil (local chain), the dstack
# simulator, deploys contracts, and launches lit-api-server / lit_actions /
# static-web-server.
#
# Runs the stack in the background and polls until healthy. Logs go to $STACK_LOG.
# Once warmed (03-warm-build.sh) this needs no internet.
#
# Env:
#   FOREGROUND=1   Run local_test.sh in the foreground (Ctrl+C to stop) instead.
#   TIMEOUT_SECS   Max seconds to wait for health (default 1800).
#
# Usage:  CHIPOTLE_DIR=/path/to/chipotle bash local-runbook/scripts/04-run-local.sh
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; source "$DIR/_common.sh"

[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env" || true
export PATH="$HOME/.foundry/bin:$HOME/.local/bin:$PATH"

CHIPOTLE_DIR="$(resolve_chipotle_dir)" || die "Chipotle checkout not found. Run 02-fetch-chipotle.sh or set CHIPOTLE_DIR."
[ -f "$SIMULATOR_DIR/dstack-simulator" ] || die "dstack-simulator missing at $SIMULATOR_DIR — run 00-install-prereqs.sh."

hd "Booting Chipotle (payment-free, local chain)"
info "checkout:  $CHIPOTLE_DIR"
info "simulator: $SIMULATOR_DIR"
info "log:       $STACK_LOG"
info "billing:   DISABLED (LIT_DISABLE_BILLING=true) — no Stripe calls"

# Already up?
if curl -sf "http://localhost:${API_PORT}/core/v1/health" >/dev/null 2>&1; then
  ok "lit-api-server already healthy on :$API_PORT — nothing to do."
  curl -s "http://localhost:${API_PORT}/core/v1/health"; echo
  exit 0
fi

export LIT_DISABLE_BILLING=true
export SIMULATOR_DIR

if [ "${FOREGROUND:-0}" = "1" ]; then
  hd "Running in foreground (Ctrl+C to stop)"
  exec bash "$CHIPOTLE_DIR/local_test.sh"
fi

info "Starting stack in the background..."
( cd "$CHIPOTLE_DIR" && LIT_DISABLE_BILLING=true SIMULATOR_DIR="$SIMULATOR_DIR" bash ./local_test.sh ) >"$STACK_LOG" 2>&1 &
STACK_PID=$!
echo "$STACK_PID" > /tmp/chipotle-local-stack.pid
info "local_test.sh PID $STACK_PID (pidfile /tmp/chipotle-local-stack.pid)"

deadline=$(( $(date +%s) + ${TIMEOUT_SECS:-1800} ))
info "Waiting for health (first run compiles Rust — can take 10–20+ min)..."
while true; do
  if curl -sf "http://localhost:${API_PORT}/core/v1/health" >/dev/null 2>&1; then
    ok "lit-api-server healthy"
    break
  fi
  if ! kill -0 "$STACK_PID" 2>/dev/null; then
    fail "local_test.sh exited before becoming healthy. Last log lines:"; tail -20 "$STACK_LOG"; exit 1
  fi
  if [ "$(date +%s)" -ge "$deadline" ]; then
    fail "Timed out waiting for health. Last log lines:"; tail -20 "$STACK_LOG"; exit 1
  fi
  sleep 5
done

hd "Stack is up"
printf "   %-22s %s\n" "chain (anvil):"   "http://127.0.0.1:${CHAIN_PORT}"
printf "   %-22s %s\n" "lit-api-server:"  "http://localhost:${API_PORT}"
printf "   %-22s %s\n" "dashboard:"       "http://localhost:${DASH_PORT}"
printf "   %-22s %s\n" "health:"          "$(curl -s http://localhost:${API_PORT}/core/v1/health)"
hd "Verify with the PKP-signing flow"
info "   CHIPOTLE_DIR=$CHIPOTLE_DIR bash local-runbook/scripts/05-verify-pkp-sign.sh"
info "Stop the stack with: bash local-runbook/scripts/06-teardown.sh"
