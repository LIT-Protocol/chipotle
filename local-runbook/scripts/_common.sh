# _common.sh — shared helpers for the Chipotle local runbook scripts.
# Sourced by the numbered scripts; not meant to be executed directly.
#
# IMPORTANT: every runbook script uses `#!/usr/bin/env bash`. Do not run them
# with zsh — zsh reserves the variable names GID/UID/EUID/EGID and assigning to
# them attempts a real setgid()/setuid() ("operation not permitted").

set -euo pipefail

# ── Colors ──────────────────────────────────────────────────────────────────
if [ -t 1 ]; then
  C_RESET=$'\033[0m'; C_BOLD=$'\033[1m'; C_RED=$'\033[31m'
  C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'; C_BLUE=$'\033[34m'
else
  C_RESET=""; C_BOLD=""; C_RED=""; C_GREEN=""; C_YELLOW=""; C_BLUE=""
fi

PASS_COUNT=0; FAIL_COUNT=0

hd()   { printf "\n${C_BOLD}${C_BLUE}== %s ==${C_RESET}\n" "$*"; }
info() { printf "   %s\n" "$*"; }
ok()   { printf "   ${C_GREEN}✅ %s${C_RESET}\n" "$*"; PASS_COUNT=$((PASS_COUNT+1)); }
warn() { printf "   ${C_YELLOW}⚠️  %s${C_RESET}\n" "$*"; }
fail() { printf "   ${C_RED}❌ %s${C_RESET}\n" "$*"; FAIL_COUNT=$((FAIL_COUNT+1)); }
die()  { printf "\n${C_RED}${C_BOLD}FATAL:${C_RESET} %s\n" "$*" >&2; exit 1; }

summary() {
  printf "\n${C_BOLD}----- summary -----${C_RESET}\n"
  printf "   passed: ${C_GREEN}%d${C_RESET}   failed: ${C_RED}%d${C_RESET}\n" "$PASS_COUNT" "$FAIL_COUNT"
  [ "$FAIL_COUNT" -eq 0 ]
}

# ── Privilege escalation (works as root-without-sudo, e.g. in containers) ─────
if [ "$(id -u)" -eq 0 ]; then SUDO=""; else SUDO="sudo"; fi
# Run a command as root: uses sudo when needed, runs directly when already root.
asroot() { if [ -n "$SUDO" ]; then sudo "$@"; else "$@"; fi; }

# ── OS detection ──────────────────────────────────────────────────────────────
detect_os() {
  case "$(uname -s)" in
    Darwin) echo "macos" ;;
    Linux)  echo "linux" ;;
    *)      echo "unsupported" ;;
  esac
}

# ── Locate the Chipotle checkout ──────────────────────────────────────────────
# Priority: $CHIPOTLE_DIR, then the repo these scripts live in, then ~/GitHub/chipotle.
resolve_chipotle_dir() {
  if [ -n "${CHIPOTLE_DIR:-}" ] && [ -f "${CHIPOTLE_DIR}/local_test.sh" ]; then
    echo "$CHIPOTLE_DIR"; return 0
  fi
  local here; here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
  if [ -f "$here/local_test.sh" ]; then echo "$here"; return 0; fi
  if [ -f "$HOME/GitHub/chipotle/local_test.sh" ]; then echo "$HOME/GitHub/chipotle"; return 0; fi
  return 1
}

# ── Config (override via env) ─────────────────────────────────────────────────
API_PORT="${API_PORT:-8000}"
DASH_PORT="${DASH_PORT:-8080}"
CHAIN_PORT="${CHAIN_PORT:-8545}"
BASE_URL_LOCAL="${BASE_URL_LOCAL:-http://localhost:${API_PORT}/core/v1}"
SIMULATOR_DIR="${SIMULATOR_DIR:-$HOME/GitHub/dstack/sdk/simulator}"
STACK_LOG="${STACK_LOG:-/tmp/chipotle-local-stack.log}"
