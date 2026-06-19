#!/usr/bin/env bash
# Regenerate the soak latency baseline used by the deploy perf gate.
#
# Runs the soak test against staging (creating its own ephemeral, test-mode-
# funded accounts via SOAK_CREATE_ACCOUNTS) and saves the per-endpoint p95
# summary to k6/baselines/soak.<network>.json. This is a MANUAL operation: run
# it when perf legitimately changes (e.g. a release made things substantially
# faster), eyeball the numbers, and commit the updated baseline in a PR. The
# deploy gate compares every staging deploy against the committed baseline.
#
# It is a MEASUREMENT run — it deliberately does NOT set SOAK_BASELINE_FILE, so
# it never gates itself; it just measures and writes the result.
#
# Requires: k6 installed locally (https://grafana.com/docs/k6/latest/set-up/install-k6/).
#
# Usage (from anywhere):
#   k6/update-soak-baseline.sh                  # network=next, 5m steady
#   DURATION=10m k6/update-soak-baseline.sh     # longer run → steadier p95
#   NETWORK=next BASE_URL=https://… k6/update-soak-baseline.sh
set -euo pipefail
cd "$(dirname "$0")" # → k6/

NETWORK="${NETWORK:-next}"
DURATION="${DURATION:-5m}"
BASE_URL="${BASE_URL:-https://test.chipotle.litprotocol.com/core/v1}"
OUT="baselines/soak.${NETWORK}.json"

command -v k6 >/dev/null 2>&1 || {
  echo "error: k6 not found — install it: https://grafana.com/docs/k6/latest/set-up/install-k6/" >&2
  exit 1
}

echo "Measuring soak baseline → ${OUT}"
echo "  target:   ${BASE_URL}"
echo "  duration: ${DURATION} steady (+~4m ramp up/down)"
echo "  accounts: ephemeral, created + test-mode-funded for this run"
echo

K6_CORRELATION_ID="baseline-$(date +%s)" \
SOAK_CREATE_ACCOUNTS=true \
SCENARIO=soak \
SOAK_VUS=2 \
SOAK_DURATION="$DURATION" \
BASE_URL="$BASE_URL" \
K6_ENV=staging \
  k6 run loadtest/soak.spec.ts

[ -f soak-summary.json ] || {
  echo "error: soak-summary.json was not produced — the run likely failed before the summary." >&2
  exit 1
}

mkdir -p baselines
mv soak-summary.json "$OUT"

echo
echo "Wrote ${OUT}:"
cat "$OUT"
echo
echo "Next: review the p95 numbers, then commit them:"
echo "  git add k6/${OUT} && git commit -m 'chore(k6): refresh soak baseline' && open a PR"
