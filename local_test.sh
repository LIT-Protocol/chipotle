#!/bin/bash
# local_test.sh — Spins up a complete local testing environment for Seville.
#
# Steps:
#   1. Start Anvil (local Ethereum node)
#   2. Start dstack simulator
#   3. Deploy contracts and generate NodeConfig.toml
#   4. Start Jaeger (docker container, UI on default port 16686)
#   5. cargo run lit-api-server
#   6. cargo run lit-actions
#   7. Serve lit-static via static-web-server
#
# Flags:
#   --no_code   Skip steps 5–7 (lit-api-server, lit-actions, lit-static).
#               Prints the cargo commands needed to start them manually,
#               with and without OTEL support.
#
# Press Ctrl+C to tear down all background processes.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# --------------------------------------------------------------------------
# Argument parsing
# --------------------------------------------------------------------------
NO_CODE=false
for arg in "$@"; do
    case "$arg" in
        --no_code|--no-code)
            NO_CODE=true
            ;;
        -h|--help)
            awk 'NR == 1 { next } /^$/ { exit } { sub(/^# ?/, ""); print }' "$0"
            exit 0
            ;;
        *)
            echo "ERROR: Unknown argument: $arg"
            echo "Usage: $0 [--no_code]"
            exit 1
            ;;
    esac
done

# Simulator path (override with SIMULATOR_DIR env var if cloned elsewhere)
SIMULATOR_DIR="${SIMULATOR_DIR:-$HOME/GitHub/dstack/sdk/simulator}"
SIMULATOR_BIN="$SIMULATOR_DIR/dstack-simulator"

# Jaeger configuration
JAEGER_CONTAINER_NAME="lit-local-jaeger"
JAEGER_UI_PORT=16686
JAEGER_OTLP_GRPC_PORT=4317
JAEGER_OTLP_HTTP_PORT=4318
JAEGER_IMAGE="jaegertracing/all-in-one:latest"

# Track background PIDs for cleanup
PIDS=()
SIM_TMP_OWNED=false
JAEGER_OWNED=false

cleanup() {
    echo ""
    echo "==> Shutting down local environment..."
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    # Only stop Jaeger container if we started it
    if [ "$JAEGER_OWNED" = true ]; then
        docker stop "$JAEGER_CONTAINER_NAME" >/dev/null 2>&1 || true
        docker rm "$JAEGER_CONTAINER_NAME" >/dev/null 2>&1 || true
    fi
    # Only clean up simulator temp dir if we created it
    if [ "$SIM_TMP_OWNED" = true ] && [ -n "${SIM_TMP:-}" ] && [ -d "${SIM_TMP:-}" ]; then
        rm -rf "$SIM_TMP"
    fi
    echo "==> All processes stopped."
}
trap cleanup EXIT INT TERM

# wait_for SERVICE_NAME TIMEOUT_SECS PID CHECK_CMD
#   Polls CHECK_CMD every 1-2s until it succeeds or TIMEOUT_SECS is reached.
#   If PID is set and the process dies, exits immediately.
wait_for() {
    local name="$1" timeout="$2" pid="${3:-}" check_cmd="$4"
    local interval=1 iterations="$timeout"
    if [ "$timeout" -gt 20 ]; then
        interval=2
        iterations=$((timeout / interval))
    fi
    for i in $(seq 1 "$iterations"); do
        if eval "$check_cmd" >/dev/null 2>&1; then
            return 0
        fi
        if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
            echo "ERROR: $name process exited unexpectedly."
            exit 1
        fi
        sleep "$interval"
    done
    return 1
}

# --------------------------------------------------------------------------
# 1. Start Anvil
# --------------------------------------------------------------------------
echo "==> Step 1: Starting Anvil..."

if ! command -v anvil &>/dev/null; then
    echo "ERROR: anvil is not installed."
    echo ""
    echo "Install Foundry (which includes anvil) by running:"
    echo "  curl -L https://foundry.paradigm.xyz | bash"
    echo "  foundryup"
    echo ""
    echo "Then re-run this script."
    exit 1
fi

anvil --silent &
ANVIL_PID=$!
PIDS+=("$ANVIL_PID")

# Wait for Anvil to be ready
if wait_for "Anvil" 10 "$ANVIL_PID" \
    'curl -sf http://127.0.0.1:8545 -X POST -H "Content-Type: application/json" -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_blockNumber\",\"params\":[],\"id\":1}"'; then
    echo "    Anvil is ready (PID $ANVIL_PID)."
else
    echo "ERROR: Anvil failed to start within 10 seconds."
    exit 1
fi

# --------------------------------------------------------------------------
# 2. Start dstack simulator
# --------------------------------------------------------------------------
echo "==> Step 2: Starting dstack simulator..."

if [ ! -f "$SIMULATOR_BIN" ]; then
    echo "ERROR: dstack-simulator not found at $SIMULATOR_BIN"
    echo ""
    echo "Clone and build the simulator:"
    echo "  git clone https://github.com/Dstack-TEE/dstack.git ~/GitHub/dstack"
    echo "  cd ~/GitHub/dstack/sdk/simulator && bash build.sh"
    echo ""
    echo "Then re-run this script."
    exit 1
fi

# Check if simulator is already running
if pgrep -x dstack-simulator >/dev/null 2>&1; then
    echo "    dstack-simulator is already running."
    if [ -n "${DSTACK_SOCKET:-}" ] && [ -S "$DSTACK_SOCKET" ]; then
        # Caller explicitly set a socket path — use it
        :
    else
        # Search /tmp for a socket created by a previous run of this script
        DSTACK_SOCKET=$(find /tmp/dstack-sim-* -name "dstack.sock" -type s 2>/dev/null | head -1)
    fi
    if [ -z "${DSTACK_SOCKET:-}" ] || [ ! -S "$DSTACK_SOCKET" ]; then
        echo "ERROR: dstack-simulator process found but no live socket detected."
        echo "       Kill the stale process (pkill dstack-simulator) and re-run."
        exit 1
    fi
    echo "    Using existing socket at $DSTACK_SOCKET"
else
    # Create isolated temp dir for simulator working files
    SIM_TMP=$(mktemp -d /tmp/dstack-sim-XXXXXX)
    SIM_TMP_OWNED=true
    cp "$SIMULATOR_DIR/appkeys.json" "$SIMULATOR_DIR/app-compose.json" \
       "$SIMULATOR_DIR/sys-config.json" "$SIMULATOR_DIR/attestation.bin" \
       "$SIMULATOR_DIR/dstack.toml" "$SIM_TMP/"

    # Rewrite the internal socket path to a user-writable location
    # (the default /var/run/dstack.sock requires root on macOS)
    DSTACK_SOCKET="$SIM_TMP/dstack.sock"
    sed -i '' "s|unix:/var/run/dstack.sock|unix:$DSTACK_SOCKET|" "$SIM_TMP/dstack.toml"

    (cd "$SIM_TMP" && exec "$SIMULATOR_BIN") >> "$SIM_TMP/dstack-simulator.log" 2>&1 &
    SIM_PID=$!
    PIDS+=("$SIM_PID")

    if wait_for "dstack-simulator" 15 "$SIM_PID" "[ -S '$DSTACK_SOCKET' ]"; then
        echo "    Simulator ready at $DSTACK_SOCKET (PID $SIM_PID)."
    else
        echo "ERROR: dstack.sock never appeared."
        echo ""
        echo "Check log: $SIM_TMP/dstack-simulator.log"
        echo "You may need to set execute permissions:"
        echo "  chmod +x $SIMULATOR_BIN"
        exit 1
    fi
fi

export DSTACK_SOCKET

# --------------------------------------------------------------------------
# 3. Deploy contracts and create NodeConfig.toml
# --------------------------------------------------------------------------
echo "==> Step 3: Deploying contracts to Anvil..."

CONTRACTS_DIR="$SCRIPT_DIR/lit-api-server/blockchain/lit_node_express"

# Run the deploy and capture output to extract the AccountConfig address
DEPLOY_OUTPUT=$(make -C "$CONTRACTS_DIR" deploy_anvil 2>&1) || {
    echo "ERROR: Contract deployment failed."
    echo "$DEPLOY_OUTPUT"
    exit 1
}
echo "$DEPLOY_OUTPUT"

# Extract the last "deployed to 0x..." address — this is the AccountConfig (diamond proxy)
CONTRACT_ADDRESS=$(echo "$DEPLOY_OUTPUT" | grep -oE 'deployed to 0x[0-9a-fA-F]+' | tail -1 | sed 's/deployed to //')

if [ -z "$CONTRACT_ADDRESS" ] || [ ${#CONTRACT_ADDRESS} -ne 42 ]; then
    echo "ERROR: Could not extract a valid contract address from deploy output."
    echo "       Got: '${CONTRACT_ADDRESS:-<empty>}' (expected 0x + 40 hex chars)"
    exit 1
fi

echo "    Diamond proxy deployed at: $CONTRACT_ADDRESS"

# Write NodeConfig.toml for lit-api-server
cat > "$SCRIPT_DIR/lit-api-server/NodeConfig.toml" <<EOF
[chain]
name = "anvil"
contract_address = "$CONTRACT_ADDRESS"
EOF
echo "    NodeConfig.toml written."

# --------------------------------------------------------------------------
# 4. Start Jaeger (docker)
# --------------------------------------------------------------------------
echo "==> Step 4: Starting Jaeger (docker)..."

if ! command -v docker &>/dev/null; then
    echo "ERROR: docker is not installed or not on PATH."
    echo ""
    echo "Install Docker Desktop from https://www.docker.com/products/docker-desktop/"
    echo "and ensure the daemon is running, then re-run this script."
    exit 1
fi

if ! docker info >/dev/null 2>&1; then
    echo "ERROR: docker daemon is not reachable. Start Docker Desktop and re-run."
    exit 1
fi

# If a container with our name already exists, reuse it (start if stopped).
if docker ps -a --format '{{.Names}}' | grep -qx "$JAEGER_CONTAINER_NAME"; then
    if docker ps --format '{{.Names}}' | grep -qx "$JAEGER_CONTAINER_NAME"; then
        echo "    Reusing running container '$JAEGER_CONTAINER_NAME'."
    else
        echo "    Starting existing container '$JAEGER_CONTAINER_NAME'..."
        docker start "$JAEGER_CONTAINER_NAME" >/dev/null
    fi
else
    echo "    Launching new Jaeger container '$JAEGER_CONTAINER_NAME'..."
    docker run -d \
        --name "$JAEGER_CONTAINER_NAME" \
        -e COLLECTOR_OTLP_ENABLED=true \
        -p "${JAEGER_UI_PORT}:16686" \
        -p "${JAEGER_OTLP_GRPC_PORT}:4317" \
        -p "${JAEGER_OTLP_HTTP_PORT}:4318" \
        "$JAEGER_IMAGE" >/dev/null
    JAEGER_OWNED=true
fi

if wait_for "Jaeger UI" 30 "" \
    "curl -sf http://localhost:${JAEGER_UI_PORT}/ -o /dev/null"; then
    echo "    Jaeger UI ready at http://localhost:${JAEGER_UI_PORT}"
    echo "    OTLP gRPC endpoint: http://127.0.0.1:${JAEGER_OTLP_GRPC_PORT}"
else
    echo "    WARNING: Jaeger UI did not respond on port ${JAEGER_UI_PORT} within 30s."
    echo "             Check 'docker logs $JAEGER_CONTAINER_NAME' for details."
fi

LIT_TELEMETRY_ENDPOINT_URL="http://127.0.0.1:${JAEGER_OTLP_GRPC_PORT}"

# --------------------------------------------------------------------------
# --no_code: print summary + cargo commands and wait
# --------------------------------------------------------------------------
if [ "$NO_CODE" = true ]; then
    echo ""
    echo "============================================"
    echo "  --no_code mode: skipping cargo services"
    echo "============================================"
    echo "  Anvil (chain):        http://127.0.0.1:8545"
    echo "  dstack simulator:     $DSTACK_SOCKET"
    echo "  Contract address:     $CONTRACT_ADDRESS"
    echo "  Jaeger UI:            http://localhost:${JAEGER_UI_PORT}"
    echo "  Jaeger OTLP (gRPC):   $LIT_TELEMETRY_ENDPOINT_URL"
    echo "============================================"
    echo ""
    echo "Run the following commands in separate terminals to start the"
    echo "remaining services. Each service has two variants:"
    echo "  (a) normal      — no telemetry export"
    echo "  (b) with OTEL   — exports traces to the local Jaeger above"
    echo ""
    echo "Required env for any cargo run:"
    echo "  export DSTACK_SOCKET=\"$DSTACK_SOCKET\""
    echo ""
    echo "--- lit-api-server ---"
    echo "  (a) Normal:"
    echo "      cd $SCRIPT_DIR/lit-api-server && \\"
    echo "          DSTACK_SOCKET=\"$DSTACK_SOCKET\" \\"
    echo "          cargo run --bin lit-api-server"
    echo ""
    echo "  (b) With OTEL → local Jaeger:"
    echo "      cd $SCRIPT_DIR/lit-api-server && \\"
    echo "          DSTACK_SOCKET=\"$DSTACK_SOCKET\" \\"
    echo "          LIT_TELEMETRY_ENDPOINT=\"$LIT_TELEMETRY_ENDPOINT_URL\" \\"
    echo "          cargo run --bin lit-api-server --features otlp"
    echo ""
    echo "--- lit-actions ---"
    echo "  (a) Normal:"
    echo "      cd $SCRIPT_DIR && \\"
    echo "          cargo run --manifest-path=lit-actions/Cargo.toml --bin lit_actions"
    echo ""
    echo "  (b) With OTEL → local Jaeger:"
    echo "      cd $SCRIPT_DIR && \\"
    echo "          LIT_TELEMETRY_ENDPOINT=\"$LIT_TELEMETRY_ENDPOINT_URL\" \\"
    echo "          cargo run --manifest-path=lit-actions/Cargo.toml --bin lit_actions \\"
    echo "                    --features otlp"
    echo ""
    echo "--- lit-static ---"
    echo "      static-web-server -p 8080 -d \"$SCRIPT_DIR/lit-static\" -g info"
    echo ""
    echo "Press Ctrl+C to stop the services started by this script"
    echo "(Anvil, dstack-simulator, Jaeger)."
    echo ""

    # Idle loop — keep Anvil/simulator/Jaeger alive until Ctrl+C.
    set +e
    while true; do
        for pid in "${PIDS[@]}"; do
            if ! kill -0 "$pid" 2>/dev/null; then
                wait "$pid" 2>/dev/null
                EXIT_CODE=$?
                EXITED_NAME="unknown"
                [ "$pid" = "${ANVIL_PID:-}" ] && EXITED_NAME="anvil"
                [ "$pid" = "${SIM_PID:-}" ] && EXITED_NAME="dstack-simulator"
                if [ "$EXIT_CODE" -eq 0 ]; then
                    echo "WARNING: $EXITED_NAME (PID $pid) exited cleanly. Shutting down."
                else
                    echo "ERROR: $EXITED_NAME (PID $pid) exited with code $EXIT_CODE"
                fi
                exit "$EXIT_CODE"
            fi
        done
        sleep 2
    done
fi

# --------------------------------------------------------------------------
# 5. cargo run lit-api-server
# --------------------------------------------------------------------------
echo "==> Step 5: Starting lit-api-server..."

(cd "$SCRIPT_DIR/lit-api-server" && \
    DSTACK_SOCKET="$DSTACK_SOCKET" \
    cargo run --bin lit-api-server) &
API_PID=$!
PIDS+=("$API_PID")

# Wait for lit-api-server to respond (up to ~120s with 2s interval)
if wait_for "lit-api-server" 60 "$API_PID" \
    'curl -sf http://localhost:8000/core/v1/health || curl -sf http://localhost:8000/'; then
    echo "    lit-api-server is ready (PID $API_PID)."
else
    echo "    WARNING: lit-api-server may still be compiling/starting. Continuing..."
fi

# --------------------------------------------------------------------------
# 6. cargo run lit-actions
# --------------------------------------------------------------------------
echo "==> Step 6: Starting lit-actions..."

(cd "$SCRIPT_DIR" && \
    cargo run --manifest-path=lit-actions/Cargo.toml --bin lit_actions) &
ACTIONS_PID=$!
PIDS+=("$ACTIONS_PID")

# Wait for the unix socket to appear (up to ~120s with 2s interval)
if wait_for "lit-actions" 60 "$ACTIONS_PID" '[ -S /tmp/lit_actions.sock ]'; then
    echo "    lit-actions is ready (PID $ACTIONS_PID)."
else
    echo "    WARNING: lit-actions may still be compiling/starting. Continuing..."
fi

# --------------------------------------------------------------------------
# 7. Serve lit-static via static-web-server
# --------------------------------------------------------------------------
echo "==> Step 7: Starting static-web-server for lit-static..."

if ! command -v static-web-server &>/dev/null; then
    echo "ERROR: static-web-server is not installed."
    echo ""
    echo "Install via Homebrew:"
    echo "  brew install static-web-server"
    echo ""
    echo "Then re-run this script."
    exit 1
fi

static-web-server -p 8080 -d "$SCRIPT_DIR/lit-static" -g info &
STATIC_PID=$!
PIDS+=("$STATIC_PID")
echo "    static-web-server serving lit-static on http://localhost:8080 (PID $STATIC_PID)."

# --------------------------------------------------------------------------
# Ready
# --------------------------------------------------------------------------
echo ""
echo "============================================"
echo "  Local environment is running!"
echo "============================================"
echo "  Anvil (chain):        http://127.0.0.1:8545"
echo "  lit-api-server:       http://localhost:8000"
echo "  lit-actions (gRPC):   /tmp/lit_actions.sock"
echo "  lit-static:           http://localhost:8080"
echo "  dstack simulator:     $DSTACK_SOCKET"
echo "  Jaeger UI:            http://localhost:${JAEGER_UI_PORT}"
echo "  Jaeger OTLP (gRPC):   $LIT_TELEMETRY_ENDPOINT_URL"
echo "  Contract address:     $CONTRACT_ADDRESS"
echo "============================================"
echo ""
echo "Press Ctrl+C to stop all services."
echo ""

# Wait for background processes and report failures.
# Uses a polling loop compatible with Bash 3.2+ (macOS default).
set +e
while true; do
    for pid in "${PIDS[@]}"; do
        if ! kill -0 "$pid" 2>/dev/null; then
            wait "$pid" 2>/dev/null
            EXIT_CODE=$?
            # Identify which service died
            EXITED_NAME="unknown"
            [ "$pid" = "${ANVIL_PID:-}" ] && EXITED_NAME="anvil"
            [ "$pid" = "${SIM_PID:-}" ] && EXITED_NAME="dstack-simulator"
            [ "$pid" = "${API_PID:-}" ] && EXITED_NAME="lit-api-server"
            [ "$pid" = "${ACTIONS_PID:-}" ] && EXITED_NAME="lit-actions"
            [ "$pid" = "${STATIC_PID:-}" ] && EXITED_NAME="static-web-server"
            if [ "$EXIT_CODE" -eq 0 ]; then
                echo "WARNING: $EXITED_NAME (PID $pid) exited cleanly. Shutting down."
            else
                echo "ERROR: $EXITED_NAME (PID $pid) exited with code $EXIT_CODE"
            fi
            exit "$EXIT_CODE"
        fi
    done
    sleep 2
done
