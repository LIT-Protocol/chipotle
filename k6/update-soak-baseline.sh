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

command -v python3 >/dev/null 2>&1 || {
  echo "error: python3 is required to validate the measured summary before writing the baseline." >&2
  exit 1
}

# No SOAK_BASELINE_FILE → not baseline mode, so the spec uses absolute ceilings.
# Set them sky-high so this measurement run genuinely never gates itself on
# latency (otherwise a legitimately-slower run would exit non-zero under set -e
# before we capture the numbers).
K6_CORRELATION_ID="baseline-$(date +%s)" \
SOAK_CREATE_ACCOUNTS=true \
SCENARIO=soak \
SOAK_VUS=2 \
SOAK_DURATION="$DURATION" \
BASE_URL="$BASE_URL" \
K6_ENV=staging \
SOAK_P95_ENCRYPT_MS=100000 \
SOAK_P95_ECDSA_MS=100000 \
  k6 run loadtest/soak.spec.ts

[ -f soak-summary.json ] || {
  echo "error: soak-summary.json was not produced — the run likely failed before the summary." >&2
  exit 1
}

# Validate the measurement before baking it into the gate's baseline: a degraded
# or partial run (failed checks, HTTP failures, or a null/zero p95 from a
# zero-sample scenario) must NOT become the accepted ceiling. Warn if it's
# meaningfully slower than the current baseline, since you normally refresh to
# TIGHTEN, not loosen.
mkdir -p baselines
python3 - "soak-summary.json" "$OUT" <<'PY'
import json, os, sys
new = json.load(open(sys.argv[1]))
problems = []
if new.get("checks_rate") != 1:
    problems.append(f"checks_rate={new.get('checks_rate')} (want 1.0)")
if new.get("http_req_failed_rate") not in (0, 0.0):
    problems.append(f"http_req_failed_rate={new.get('http_req_failed_rate')} (want 0)")
for name in ("soak_encrypt_decrypt", "soak_ecdsa_sign"):
    sc = (new.get("scenarios") or {}).get(name)
    p95 = sc.get("p95") if isinstance(sc, dict) else None
    if not isinstance(p95, (int, float)) or isinstance(p95, bool) or p95 <= 0:
        problems.append(f"{name}.p95={p95!r} (want a positive number)")
if problems:
    sys.stderr.write("error: measurement run was not clean — baseline NOT written:\n")
    for p in problems:
        sys.stderr.write(f"  - {p}\n")
    sys.exit(1)
cur_path = sys.argv[2]
if os.path.exists(cur_path):
    cur = json.load(open(cur_path))
    for name in ("soak_encrypt_decrypt", "soak_ecdsa_sign"):
        old = ((cur.get("scenarios") or {}).get(name) or {}).get("p95")
        nw = new["scenarios"][name]["p95"]
        if isinstance(old, (int, float)) and not isinstance(old, bool) and nw > old * 1.1:
            sys.stderr.write(
                f"WARNING: {name} p95 {nw:.0f}ms is >10% slower than the current "
                f"baseline {old:.0f}ms — refreshes usually TIGHTEN. Double-check before committing.\n"
            )
PY

mv soak-summary.json "$OUT"

echo
echo "Wrote ${OUT}:"
cat "$OUT"
echo
echo "Next: review the p95 numbers, then commit them:"
echo "  git add k6/${OUT} && git commit -m 'chore(k6): refresh soak baseline' && open a PR"
