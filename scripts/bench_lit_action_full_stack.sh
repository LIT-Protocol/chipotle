#!/usr/bin/env bash
set -euo pipefail

# Benchmark the full local_test.sh stack for the reported Lit Action.
# Creates temporary git worktrees for before/after refs, starts each stack,
# registers/funds the dstack-derived api_payers, creates an account + usage key,
# then measures POST /core/v1/lit_action wall time and the action's internal
# getPrivateKey timing.
#
# Usage:
#   scripts/bench_lit_action_full_stack.sh origin/main pr-388 10
#
# Logs/results are written under target/lit-action-full-stack-perf/.

BEFORE_REF="${1:-origin/main}"
AFTER_REF="${2:-pr-388}"
ITERATIONS="${3:-10}"
ROOT="$(git rev-parse --show-toplevel)"
OUT_DIR="$ROOT/target/lit-action-full-stack-perf"
GIST_URL="https://gist.githubusercontent.com/GTC6244/a9bbcb02aedabb3b885e462047612667/raw/5c899ecc97e4c1cd22353e1113bc0bc352f80edf/gistfile1.txt"
ANVIL_OWNER_PK="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"

export PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-1.92}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"

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

stop_stack() {
  local pid="${1:-}"
  if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
    kill -- -"$pid" >/dev/null 2>&1 || kill "$pid" >/dev/null 2>&1 || true
    sleep 2
    kill -9 -- -"$pid" >/dev/null 2>&1 || true
  fi
}

free_ports() {
  # local_test defaults to these ports. Kill stale processes from previous runs.
  for pat in 'lit-api-server' 'static-web-server' 'anvil' 'dstack-simulator'; do
    pkill -f "$pat" >/dev/null 2>&1 || true
  done
  sleep 2
}

function patch_local_test_for_linux() {
  local wt="$1"
  node - "$wt/local_test.sh" <<'JS'
const fs = require('fs');
const file = process.argv[2];
let s = fs.readFileSync(file, 'utf8');
// Make the dstack socket rewrite portable. Some refs use SIM_CONFIG, others
// edit $SIM_TMP/dstack.toml directly; some include a trailing sed `g`, others do not.
s = s.replace(
  /sed -i '' "s\|unix:\/var\/run\/dstack\.sock\|unix:\$DSTACK_SOCKET\|g?" "(?:\$SIM_CONFIG|\$SIM_TMP\/dstack\.toml)"/g,
  'perl -0pi -e "s#unix:(?:/var/run/dstack\\.sock|\\./dstack\\.sock)#unix:$DSTACK_SOCKET#g" "$SIM_TMP/dstack.toml"'
);
s = s.replace(
  'static-web-server -p 8080 -d "$SCRIPT_DIR/lit-static" -g info &',
  'static-web-server -a 127.0.0.1 -p 8080 -d "$SCRIPT_DIR/lit-static" -g info &'
);
s = s.replace(
  /# Wait for lit-api-server to respond[\s\S]*?echo "    WARNING: lit-api-server may still be compiling\/starting\. Continuing\.\.\."\nfi/,
  `# Wait for lit-api-server to respond (up to ~600s with 2s interval)
if wait_for "lit-api-server" 300 "$API_PID" \\
    'curl -sf http://localhost:8000/core/v1/health || curl -sf http://localhost:8000/'; then
    echo "    lit-api-server is ready (PID $API_PID)."
else
    echo "ERROR: lit-api-server failed to become ready."
    exit 1
fi`
);
const marker = 'echo "    NodeConfig.toml written."\n\n# --------------------------------------------------------------------------\n# 4. Start Jaeger (docker)';
const provision = `echo "    NodeConfig.toml written."

# --------------------------------------------------------------------------
# 3b. Provision local API payers before lit-api-server starts
# --------------------------------------------------------------------------
echo "==> Step 3b: Provisioning local API payers..."
if ! command -v cast &>/dev/null; then echo "ERROR: cast is not installed."; exit 1; fi
if ! command -v jq &>/dev/null; then echo "ERROR: jq is not installed."; exit 1; fi
ANVIL_OWNER_PRIVATE_KEY="\${ANVIL_OWNER_PRIVATE_KEY:-0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80}"
ADMIN_API_PAYER_KEY=\$(curl -sS --unix-socket "$DSTACK_SOCKET" -X POST http://dstack/GetKey -H 'Content-Type: application/json' -d '{"path":"v1/admin_api_payer","purpose":"lit_payer"}' | jq -r '.key')
ADMIN_API_PAYER_SECRET=\$(cast keccak "0x$ADMIN_API_PAYER_KEY")
ADMIN_API_PAYER_ADDRESS=\$(cast wallet address --private-key "$ADMIN_API_PAYER_SECRET")
REQUESTED_API_PAYER_COUNT=\$(cast call "$CONTRACT_ADDRESS" 'requestedApiPayerCount()(uint256)' --rpc-url http://127.0.0.1:8545 | cast to-dec)
if [ -z "$REQUESTED_API_PAYER_COUNT" ] || [ "$REQUESTED_API_PAYER_COUNT" -lt 1 ]; then REQUESTED_API_PAYER_COUNT=3; fi
API_PAYER_ADDRESSES=()
for payer_number in \$(seq 1 "$REQUESTED_API_PAYER_COUNT"); do
    PAYER_KEY=\$(curl -sS --unix-socket "$DSTACK_SOCKET" -X POST http://dstack/GetKey -H 'Content-Type: application/json' -d "{\\"path\\":\\"v1/payer_\${payer_number}\\",\\"purpose\\":\\"lit_payer\\"}" | jq -r '.key')
    PAYER_SECRET=\$(cast keccak "0x$PAYER_KEY")
    API_PAYER_ADDRESSES+=("\$(cast wallet address --private-key "$PAYER_SECRET")")
done
API_PAYER_ARRAY="[\$(IFS=,; echo "\${API_PAYER_ADDRESSES[*]}")]"
cast send "$CONTRACT_ADDRESS" 'setAdminApiPayerAccount(address)' "$ADMIN_API_PAYER_ADDRESS" --rpc-url http://127.0.0.1:8545 --private-key "$ANVIL_OWNER_PRIVATE_KEY" >/dev/null
cast send "$CONTRACT_ADDRESS" 'setApiPayers(address[])' "$API_PAYER_ARRAY" --rpc-url http://127.0.0.1:8545 --private-key "$ANVIL_OWNER_PRIVATE_KEY" >/dev/null
cast send "$ADMIN_API_PAYER_ADDRESS" --value 10ether --rpc-url http://127.0.0.1:8545 --private-key "$ANVIL_OWNER_PRIVATE_KEY" >/dev/null
for payer_address in "\${API_PAYER_ADDRESSES[@]}"; do
    cast send "$payer_address" --value 10ether --rpc-url http://127.0.0.1:8545 --private-key "$ANVIL_OWNER_PRIVATE_KEY" >/dev/null
done
unset ADMIN_API_PAYER_KEY ADMIN_API_PAYER_SECRET PAYER_KEY PAYER_SECRET
echo "    Provisioned \${#API_PAYER_ADDRESSES[@]} API payers and admin API payer."

# --------------------------------------------------------------------------
# 4. Start Jaeger (docker)`;
s = s.replace(marker, provision);
fs.writeFileSync(file, s);
JS
}

wait_for_stack() {
  local deadline=$((SECONDS + 420))
  until curl -sf http://127.0.0.1:8000/core/v1/health >/dev/null 2>&1; do
    if (( SECONDS > deadline )); then
      echo "timed out waiting for lit-api-server health" >&2
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
  # local_test.sh is patched to provision the dstack-derived admin/API payers
  # before lit-api-server starts, so the signer pool initializes with matching
  # signers. Verify that state instead of mutating api_payers after startup.
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
  local ref_name="$1"
  local out_json="$2"
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
  const acct = await request('new_account', { account_name: 'bench', account_description: 'full stack lit action bench' });
  const usage = await request('add_usage_api_key', {
    name: 'bench-usage', description: 'bench usage',
    can_create_groups: false, can_delete_groups: false, can_create_pkps: false,
    manage_ipfs_ids_in_groups: [], add_pkp_to_groups: [], remove_pkp_from_groups: [], execute_in_groups: [0],
  }, acct.api_key);
  const usageKey = usage.usage_api_key;
  const samples = [];
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

bench_ref() {
  local label="$1"
  local ref="$2"
  local wt="$OUT_DIR/worktree-$label"
  local log="$OUT_DIR/$label-stack.log"
  local json="$OUT_DIR/$label-results.json"
  local sha
  sha=$(git rev-parse "$ref")
  echo "==> $label $ref ($sha)"
  git worktree prune >/dev/null 2>&1 || true
  git worktree remove --force "$wt" >/dev/null 2>&1 || true
  rm -rf "$wt"
  git worktree add --detach "$wt" "$sha" >/dev/null
  patch_local_test_for_linux "$wt"
  echo "Prebuilding $label services serially..."
  ( cd "$wt/lit-api-server/blockchain/rust_generator_and_deployer" && cargo build --bin contract_deployer ) >>"$log" 2>&1
  ( cd "$wt/lit-actions" && cargo build --bin lit_actions ) >>"$log" 2>&1
  ( cd "$wt/lit-api-server" && cargo build --bin lit-api-server ) >>"$log" 2>&1
  free_ports
  # Benchmarks measure Lit Action latency, not billing; run payment-free so the
  # CPL-330 test-Stripe-key requirement doesn't block stack startup.
  ( cd "$wt" && LIT_DISABLE_BILLING=true setsid ./local_test.sh >>"$log" 2>&1 ) &
  local stack_pid=$!
  trap "stop_stack $stack_pid; git worktree remove --force '$wt' >/dev/null 2>&1 || true" RETURN
  wait_for_stack || { tail -200 "$log" >&2 || true; return 1; }
  setup_api_payers
  run_measurement "$ref" "$json"
  stop_stack "$stack_pid"
  git worktree remove --force "$wt" >/dev/null 2>&1 || true
  trap - RETURN
  jq --arg lbl "$label" --arg ref "$ref" --arg sha "$sha" '. + {"label":$lbl, "ref":$ref, "sha":$sha}' "$json" > "$json.tmp" && mv "$json.tmp" "$json"
}

if [[ "${ONLY_AFTER:-false}" != "true" ]]; then
  bench_ref before "$BEFORE_REF"
fi
if [[ "${ONLY_BEFORE:-false}" != "true" ]]; then
  bench_ref after "$AFTER_REF"
fi

if [[ -f "$OUT_DIR/before-results.json" && -f "$OUT_DIR/after-results.json" ]]; then
node - "$OUT_DIR/before-results.json" "$OUT_DIR/after-results.json" "$OUT_DIR/summary.json" <<'JS'
const fs = require('fs');
const before = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const after = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'));
function delta(a, b) { return { before: a, after: b, delta_ms: b - a, delta_pct: ((b - a) / a) * 100, speedup: a / b }; }
const summary = {
  before: { ref: before.ref, sha: before.sha },
  after: { ref: after.ref, sha: after.sha },
  iterations: before.iterations,
  wall_mean: delta(before.wall_ms.mean, after.wall_ms.mean),
  wall_p50: delta(before.wall_ms.p50, after.wall_ms.p50),
  get_private_key_mean: delta(before.get_private_key_ms.mean, after.get_private_key_ms.mean),
  get_private_key_p50: delta(before.get_private_key_ms.p50, after.get_private_key_ms.p50),
};
fs.writeFileSync(process.argv[4], JSON.stringify(summary, null, 2));
console.log(JSON.stringify(summary, null, 2));
JS
fi

echo "Wrote results to $OUT_DIR"
