#!/usr/bin/env bash
# Disposable local account-access stack. Build prerequisites first (README.md).
# No lit-actions worker is needed: these tests don't execute Actions.
# Password login uses the real lit-payments auth service against a fresh local
# Postgres database (TEST_DATABASE_URL, default postgres://localhost:5432/postgres)
# with outbound email captured by a local fake Resend endpoint.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SIMULATOR_DIR="${SIMULATOR_DIR:-$HOME/GitHub/dstack/sdk/simulator}"
CONTRACTS_DIR="$REPO_DIR/lit-api-server/blockchain/lit_node_express"
API_BIN="$REPO_DIR/lit-api-server/target/debug/lit-api-server"
PAYMENTS_BIN="${LIT_PAYMENTS_BIN:-$REPO_DIR/lit-payments/target/debug/lit-payments}"
DEPLOYER_BIN="$REPO_DIR/lit-api-server/blockchain/rust_generator_and_deployer/target/debug/contract_deployer"
LOG_DIR="$REPO_DIR/e2e/artifacts/auth-stack"
RUN_DIR=$(mktemp -d /tmp/lit-auth-XXXXXX)
PIDS=()

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  # macOS's Bash 3 treats an empty array as unset under nounset.
  set +u
  for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
  for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
  mkdir -p "$LOG_DIR"
  for log in "$RUN_DIR"/*.log; do
    [ ! -f "$log" ] || cp "$log" "$LOG_DIR/"
  done
  if [ "$status" -ne 0 ]; then
    for log in "$RUN_DIR"/*.log; do
      [ ! -f "$log" ] || tail -n 40 "$log"
    done
  fi
  rm -rf "$RUN_DIR"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for binary in "$API_BIN" "$DEPLOYER_BIN" "$PAYMENTS_BIN" "$SIMULATOR_DIR/dstack-simulator"; do
  [ -x "$binary" ] || { echo "Missing $binary — see e2e/README.md" >&2; exit 1; }
done

# Refuse to reuse a developer's running chain/API or overwrite their config.
python3 - <<'PY'
import socket
for port in (8545, 8000, 8088, 8787, 8790):
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', port))
PY

wait_for() {
  local pid=$1
  shift
  for _ in $(seq 1 60); do
    kill -0 "$pid" 2>/dev/null || { echo "Service exited during startup" >&2; return 1; }
    if "$@" >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  echo "Service readiness timed out" >&2
  return 1
}

anvil --host 127.0.0.1 --port 8545 --silent > "$RUN_DIR/anvil.log" 2>&1 &
PIDS+=("$!")
wait_for "${PIDS[0]}" cast block-number --rpc-url http://127.0.0.1:8545

cp "$SIMULATOR_DIR"/{appkeys.json,app-compose.json,sys-config.json,attestation.bin,dstack.toml} "$RUN_DIR/"
export DSTACK_SOCKET="$RUN_DIR/dstack.sock"
python3 - "$RUN_DIR/dstack.toml" "$DSTACK_SOCKET" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
text = path.read_text()
for old in ('unix:/var/run/dstack.sock', 'unix:./dstack.sock'):
    text = text.replace(old, 'unix:' + sys.argv[2])
path.write_text(text)
PY
(cd "$RUN_DIR" && exec "$SIMULATOR_DIR/dstack-simulator") > "$RUN_DIR/simulator.log" 2>&1 &
PIDS+=("$!")
wait_for "${PIDS[1]}" test -S "$DSTACK_SOCKET"

# Anvil's public test key, never a funded production wallet.
ANVIL_TEST_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
(cd "$CONTRACTS_DIR" && "$DEPLOYER_BIN" --action=deploy --network=anvil \
  --abifolder=artifacts/contracts --secret="$ANVIL_TEST_KEY") > "$RUN_DIR/deploy.log" 2>&1
CONTRACT_ADDRESS=$(sed -nE 's/.*deployed to (0x[0-9a-fA-F]{40}).*/\1/p' "$RUN_DIR/deploy.log" | tail -1)
[[ "$CONTRACT_ADDRESS" =~ ^0x[0-9a-fA-F]{40}$ ]] || { echo 'No deployed contract address' >&2; exit 1; }
cat > "$RUN_DIR/NodeConfig.toml" <<EOF
[chain]
name = "anvil"
contract_address = "$CONTRACT_ADDRESS"
EOF

# Same derivation/provisioning as local_test.sh; the server must be allowed to
# create managed accounts and have local gas before it starts.
derive_payer() {
  local key secret
  key=$(curl --fail --silent --show-error --max-time 10 --unix-socket "$DSTACK_SOCKET" \
    -H 'Content-Type: application/json' http://dstack/GetKey \
    -d "{\"path\":\"$1\",\"purpose\":\"lit_payer\"}" | jq -er '.key')
  secret=$(cast keccak "0x$key")
  cast wallet address --private-key "$secret"
}
send() {
  cast send --rpc-url http://127.0.0.1:8545 --private-key "$ANVIL_TEST_KEY" "$@" > /dev/null
}
ADMIN_PAYER=$(derive_payer v1/admin_api_payer)
send "$CONTRACT_ADDRESS" 'setAdminApiPayerAccount(address)' "$ADMIN_PAYER"
send "$ADMIN_PAYER" --value 10ether
PAYER_COUNT=$(cast call "$CONTRACT_ADDRESS" 'requestedApiPayerCount()(uint256)' --rpc-url http://127.0.0.1:8545 | cast to-dec)
[ "$PAYER_COUNT" -ge 1 ] || { echo 'Contract requested no API payers' >&2; exit 1; }
PAYERS=()
for number in $(seq 1 "$PAYER_COUNT"); do
  payer=$(derive_payer "v1/payer_$number")
  PAYERS+=("$payer")
  send "$payer" --value 10ether
done
PAYER_ARRAY="[$(IFS=,; echo "${PAYERS[*]}")]"
send "$CONTRACT_ADDRESS" 'setApiPayers(address[])' "$PAYER_ARRAY"

(cd "$RUN_DIR" && LIT_DISABLE_BILLING=true ROCKET_ADDRESS=127.0.0.1 ROCKET_PORT=8000 \
  RUST_LOG=info exec "$API_BIN") > "$RUN_DIR/api.log" 2>&1 &
PIDS+=("$!")
# /health also checks the unrelated Action worker. Probe account configuration.
wait_for "${PIDS[2]}" curl --fail --silent --max-time 3 http://localhost:8000/core/v1/get_node_chain_config

# Start static hosting last: Playwright's readiness probe now implies the API
# and chain are ready too. Keep all runtime files outside the working tree.
# Real lit-payments auth service + Postgres; only outbound email is captured.
DASHBOARD_TEST_PORT=8088 LIT_PAYMENTS_BIN="$PAYMENTS_BIN" node "$REPO_DIR/lit-dashboard-auth/test/browser-server.mjs" > "$RUN_DIR/auth-storage.log" 2>&1 &
PIDS+=("$!")
wait_for "${PIDS[3]}" curl --fail --silent --max-time 3 http://localhost:8787/health
while true; do
  for pid in "${PIDS[@]}"; do
    kill -0 "$pid" 2>/dev/null || { echo 'Account-access service stopped' >&2; exit 1; }
  done
  sleep 1
done
