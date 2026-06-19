#!/usr/bin/env bash
set -euo pipefail

# Benchmark the full local_test.sh stack for the reported Lit Action using the
# current checkout. Unlike bench_lit_action_full_stack.sh, this script does not
# create git worktrees or compare two refs; it just measures whatever code is in
# the current repository.
#
# Usage:
#   scripts/bench_lit_action_current.sh [iterations]
#
# Optional environment:
#   OUT_DIR=target/lit-action-current-perf
#   SKIP_PREBUILD=true   # skip serial cargo prebuilds
#   RUSTUP_TOOLCHAIN=1.92
#   CARGO_BUILD_JOBS=1
#
# Logs/results are written under target/lit-action-current-perf/ by default.

ITERATIONS="${1:-10}"
ROOT="$(git rev-parse --show-toplevel)"
OUT_DIR="${OUT_DIR:-$ROOT/target/lit-action-current-perf}"
GIST_URL="https://gist.githubusercontent.com/GTC6244/a9bbcb02aedabb3b885e462047612667/raw/5c899ecc97e4c1cd22353e1113bc0bc352f80edf/gistfile1.txt"

export PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-1.92}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

mkdir -p "$OUT_DIR"

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "missing required command: $1" >&2; exit 1; }
}

need_cmd git
need_cmd node
need_cmd curl
need_cmd jq
need_cmd cast
need_cmd anvil
need_cmd forge
need_cmd static-web-server
if [[ ! -x "$HOME/GitHub/dstack/sdk/simulator/dstack-simulator" ]]; then
  echo "missing dstack simulator at $HOME/GitHub/dstack/sdk/simulator/dstack-simulator" >&2
  exit 1
fi

cleanup_local_stack() {
  # Keep this intentionally broad: local_test.sh uses fixed local ports/service
  # names, so stale local benchmark processes will break the next run anyway.
  for pat in 'lit-api-server' 'lit_actions' 'static-web-server' 'anvil' 'dstack-simulator'; do
    pkill -f "$pat" >/dev/null 2>&1 || true
  done
}

stop_stack() {
  local pid="${1:-}"
  if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
    kill -- -"$pid" >/dev/null 2>&1 || kill "$pid" >/dev/null 2>&1 || true
    sleep 2
    kill -9 -- -"$pid" >/dev/null 2>&1 || true
  fi
  cleanup_local_stack
}

free_ports() {
  cleanup_local_stack
  sleep 2
}

wait_for_stack() {
  local log="$1"
  local stack_pid="$2"
  local deadline=$((SECONDS + 900))
  until curl -sf http://127.0.0.1:8000/core/v1/health >/dev/null 2>&1; do
    if ! kill -0 "$stack_pid" >/dev/null 2>&1; then
      echo "local_test.sh exited before lit-api-server became healthy" >&2
      tail -200 "$log" >&2 || true
      return 1
    fi
    if (( SECONDS > deadline )); then
      echo "timed out waiting for lit-api-server health" >&2
      tail -200 "$log" >&2 || true
      return 1
    fi
    sleep 2
  done
  node - <<'JS'
(async () => {
  const deadline = Date.now() + 600_000;
  while (Date.now() < deadline) {
    try {
      const h = await (await fetch('http://127.0.0.1:8000/core/v1/health')).json();
      if (h.lit_actions_reachable && h.cpu_available) process.exit(0);
    } catch {}
    await new Promise(r => setTimeout(r, 2000));
  }
  process.exit(1);
})();
JS
}

setup_api_payers() {
  # local_test.sh should provision dstack-derived admin/API payers before
  # lit-api-server starts. Verify that state before measuring.
  node - <<'JS'
(async () => {
  const res = await fetch('http://127.0.0.1:8000/core/v1/get_api_payers');
  if (!res.ok) throw new Error(`/get_api_payers ${res.status}: ${await res.text()}`);
  const payers = await res.json();
  if (!Array.isArray(payers) || payers.length === 0) {
    throw new Error('/get_api_payers returned no payers; local_test provisioning failed');
  }
  console.log(`verified ${payers.length} api_payers`);
})().catch(e => { console.error(e); process.exit(1); });
JS
}

run_measurement() {
  local out_json="$1"
  local action_file="$OUT_DIR/action.js"
  curl -fsSL "$GIST_URL" -o "$action_file"
  node - "$ITERATIONS" "$action_file" "$out_json" <<'JS'
const fs = require('fs');
const iterations = Number(process.argv[2]);
const actionFile = process.argv[3];
const outJson = process.argv[4];
const code = fs.readFileSync(actionFile, 'utf8');
async function request(path, body, apiKey) {
  const headers = { 'Content-Type': 'application/json' };
  if (apiKey) headers['X-Api-Key'] = apiKey;
  const res = await fetch(`http://127.0.0.1:8000/core/v1/${path}`, { method: 'POST', headers, body: JSON.stringify(body) });
  const text = await res.text();
  let data; try { data = JSON.parse(text); } catch { data = text; }
  if (!res.ok) throw new Error(`${path} HTTP ${res.status}: ${text}`);
  return data;
}
function percentile(xs, p) {
  const s = [...xs].sort((a,b)=>a-b);
  return s[Math.min(s.length - 1, Math.floor((s.length - 1) * p))];
}
(async () => {
  const acct = await request('new_account', { account_name: 'bench-current', account_description: 'current checkout lit action bench' });
  const usage = await request('add_usage_api_key', {
    name: 'bench-current-usage', description: 'bench current usage',
    can_create_groups: false, can_delete_groups: false, can_create_pkps: false,
    manage_ipfs_ids_in_groups: [], add_pkp_to_groups: [], remove_pkp_from_groups: [], execute_in_groups: [0],
  }, acct.api_key);
  const usageKey = usage.usage_api_key;
  const samples = [];
  // One warmup request, then N measured requests.
  for (let i = 0; i < iterations + 1; i++) {
    const message = '0x' + Buffer.alloc(32, i).toString('hex');
    const t0 = process.hrtime.bigint();
    const data = await request('lit_action', {
      code,
      js_params: { pkpId: acct.wallet_address, action: 'sign', vmType: 'ethereum-vm', message },
    }, usageKey);
    const wallMs = Number(process.hrtime.bigint() - t0) / 1e6;
    const address = data?.response?.address ?? '';
    const m = /Signature took ([0-9.]+) milliseconds/.exec(address);
    if (i > 0) samples.push({ wall_ms: wallMs, get_private_key_ms: m ? Number(m[1]) : null, has_error: data.has_error });
  }
  const wall = samples.map(s => s.wall_ms);
  const inner = samples.map(s => s.get_private_key_ms).filter(x => x != null);
  const result = {
    iterations,
    wall_ms: { mean: wall.reduce((a,b)=>a+b,0)/wall.length, min: Math.min(...wall), p50: percentile(wall, .5), p95: percentile(wall, .95), max: Math.max(...wall) },
    get_private_key_ms: { mean: inner.reduce((a,b)=>a+b,0)/inner.length, min: Math.min(...inner), p50: percentile(inner, .5), p95: percentile(inner, .95), max: Math.max(...inner) },
    errors: samples.filter(s => s.has_error).length,
  };
  fs.writeFileSync(outJson, JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
})().catch(e => { console.error(e); process.exit(1); });
JS
}

LOG="$OUT_DIR/current-stack.log"
JSON="$OUT_DIR/current-results.json"
SUMMARY="$OUT_DIR/current-summary.json"
REF="$(git branch --show-current || true)"
SHA="$(git rev-parse HEAD)"
DIRTY="$(git status --porcelain | wc -l | tr -d ' ')"

echo "==> Benchmarking current checkout ${REF:-detached} ($SHA), dirty_files=$DIRTY"
: > "$LOG"

if [[ "${SKIP_PREBUILD:-false}" != "true" ]]; then
  echo "Prebuilding services serially..."
  ( cd "$ROOT/lit-api-server/blockchain/rust_generator_and_deployer" && cargo build --bin contract_deployer ) >>"$LOG" 2>&1
  ( cd "$ROOT/lit-actions" && cargo build --bin lit_actions ) >>"$LOG" 2>&1
  ( cd "$ROOT/lit-api-server" && cargo build --bin lit-api-server ) >>"$LOG" 2>&1
fi

free_ports
# Benchmarks measure Lit Action latency, not billing; run payment-free so the
# CPL-330 test-Stripe-key requirement doesn't block stack startup.
( cd "$ROOT" && LIT_DISABLE_BILLING=true setsid ./local_test.sh >>"$LOG" 2>&1 ) &
STACK_PID=$!
trap 'stop_stack "$STACK_PID"' EXIT

wait_for_stack "$LOG" "$STACK_PID"
setup_api_payers
run_measurement "$JSON"
stop_stack "$STACK_PID"
trap - EXIT

jq --arg ref "$REF" --arg sha "$SHA" --arg dirty "$DIRTY" '. + {"ref":$ref, "sha":$sha, "dirty_files":($dirty|tonumber)}' "$JSON" > "$JSON.tmp" && mv "$JSON.tmp" "$JSON"
cp "$JSON" "$SUMMARY"
cat "$SUMMARY"
echo "Wrote results to $OUT_DIR"
