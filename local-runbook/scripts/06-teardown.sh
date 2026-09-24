#!/usr/bin/env bash
# 06-teardown.sh — stop the local Chipotle stack and clean up stray processes.
# local_test.sh traps Ctrl+C, but when started in the background (04-run-local.sh)
# it can leave children behind. This kills them all.
#
# Usage:  bash local-runbook/scripts/06-teardown.sh
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; source "$DIR/_common.sh" || true
set +e

hd "Tearing down local Chipotle stack"

# 1. Signal the launcher (so its own trap cleans up jaeger/simulator tmp).
if [ -f /tmp/chipotle-local-stack.pid ]; then
  PID="$(cat /tmp/chipotle-local-stack.pid 2>/dev/null)"
  if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
    info "Sending TERM to local_test.sh (PID $PID)..."
    kill "$PID" 2>/dev/null
    sleep 2
  fi
  rm -f /tmp/chipotle-local-stack.pid
fi

# 2. Kill remaining stack processes. TERM first (graceful), then escalate to KILL
#    for anything that ignores TERM (the Rust services do a slow graceful shutdown).
PATTERNS=('local_test.sh' 'target/debug/lit-api-server' 'target/debug/lit_actions' 'lit-api-server' 'lit_actions')
NAMES=(anvil dstack-simulator static-web-server)

for pat in "${PATTERNS[@]}"; do pkill -TERM -f "$pat" 2>/dev/null && info "TERM: $pat" || true; done
for name in "${NAMES[@]}"; do pkill -TERM -x "$name" 2>/dev/null && info "TERM: $name" || true; done
sleep 3
for pat in "${PATTERNS[@]}"; do pkill -KILL -f "$pat" 2>/dev/null && info "KILL: $pat" || true; done
for name in "${NAMES[@]}"; do pkill -KILL -x "$name" 2>/dev/null && info "KILL: $name" || true; done

# 3. Optional Jaeger container from local_test.sh.
if command -v docker >/dev/null 2>&1 && docker ps -a --format '{{.Names}}' 2>/dev/null | grep -qx lit-local-jaeger; then
  docker stop lit-local-jaeger >/dev/null 2>&1 && docker rm lit-local-jaeger >/dev/null 2>&1 && info "removed jaeger container" || true
fi

sleep 1
# Verify nothing is *actually* alive. Ignore zombies (STAT Z): a killed child whose
# parent has exited lingers as a zombie until PID 1 reaps it — harmless (holds no
# ports/CPU). Under a non-reaping container PID 1 (e.g. `sleep infinity`) zombies
# persist until the container restarts; run the container with `--init` to auto-reap.
LEFT=0; ZOMBIES=0
for name in anvil dstack-simulator static-web-server lit_actions lit-api-server; do
  for pid in $(pgrep -f "$name" 2>/dev/null); do
    st="$(ps -o stat= -p "$pid" 2>/dev/null | tr -d ' ')"
    case "$st" in
      Z*|'') ZOMBIES=$((ZOMBIES+1)) ;;              # dead, awaiting reap — fine
      *)     warn "still running: $name (pid $pid, stat $st)"; LEFT=1 ;;
    esac
  done
done
if [ "$LEFT" -eq 0 ]; then
  ok "all stack processes stopped"
  [ "$ZOMBIES" -gt 0 ] && info "($ZOMBIES zombie(s) awaiting reap by PID 1 — harmless; gone on container restart / use 'docker run --init')"
else
  warn "some processes remain — inspect with: pgrep -fl 'anvil|dstack|lit_actions|lit-api-server|static-web-server'"
fi

hd "Done"
