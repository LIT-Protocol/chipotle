# Conventions: https://github.com/casey/just

image_base := env('DOCKER_IMAGE', 'litptcl/lit-node-express')
# Tag used only to satisfy the registry push requirement; the deploy step uses
# the @sha256: digest captured after push, never this tag. Override with
# DOCKER_TAG to reuse a previously-pushed image (digest files must then exist).
image_tag := env('DOCKER_TAG', `uuidgen | tr '[:upper:]' '[:lower:]' | tr -d '\n'`)
image_lit_actions    := image_base + '-lit-actions:'    + image_tag
image_lit_api_server := image_base + '-lit-api-server:' + image_tag
image_lit_static     := image_base + '-lit-static:'     + image_tag
# main → lit-api-server; any other branch → lit-api-server-next (override with PHALA_APP_NAME)
app_name := `git branch --show-current | xargs -I {} sh -c '[ "{}" = "main" ] && echo lit-api-server || echo lit-api-server-next'`
instance_type := `git branch --show-current | xargs -I {} sh -c '[ "{}" = "main" ] && echo tdx.large || echo tdx.small'`

# List available recipes (default when invoked with no args)
default:
    @just --list

[group: 'build']
build: actions api-server

[group: 'build']
actions:
    cargo build --manifest-path=lit-actions/Cargo.toml --bin lit_actions --all-features

[group: 'build']
api-server:
    cargo build --manifest-path=lit-api-server/Cargo.toml --bin lit-api-server --all-features

clean:
    cargo clean --manifest-path=lit-actions/Cargo.toml
    cargo clean --manifest-path=lit-api-server/Cargo.toml
# Install Phala CLI (optional; run before first deploy)

[group: 'deploy']
setup:
    #!/usr/bin/env sh
    set -eu
    command -v npm >/dev/null 2>&1 || { echo "error: npm not found. Install Node.js from https://nodejs.org/"; exit 1; }
    npm install -g phala
    phala --version

# Build all three Docker images in parallel (release mode, linux/amd64 for Phala CVM)
[group: 'deploy']
docker-build: _check_docker
    #!/usr/bin/env sh
    set -eu
    docker build --platform linux/amd64 -f Dockerfile.lit-actions    -t {{image_lit_actions}}    . &
    docker build --platform linux/amd64 -f Dockerfile.lit-api-server -t {{image_lit_api_server}} . &
    docker build --platform linux/amd64 -f Dockerfile.lit-static     -t {{image_lit_static}}     . &
    wait

# Push all three images in parallel and capture each registry-assigned @sha256: digest.
# Digests are written to .digest-{service}.txt and read by `deploy` (DR-1.1, DR-1.2).
# docker inspect .RepoDigests is populated by `docker push` with the registry-assigned
# content hash; we strip the repo prefix to get sha256:...
[group: 'deploy']
docker-push: docker-build
    #!/usr/bin/env sh
    set -eu
    docker push {{image_lit_actions}}    &
    docker push {{image_lit_api_server}} &
    docker push {{image_lit_static}}     &
    wait
    docker inspect --format='{{{{json .RepoDigests}}}}' {{image_lit_actions}}    | tr -d '[]" ' | cut -d',' -f1 | sed 's/.*@//' > .digest-lit-actions.txt
    docker inspect --format='{{{{json .RepoDigests}}}}' {{image_lit_api_server}} | tr -d '[]" ' | cut -d',' -f1 | sed 's/.*@//' > .digest-lit-api-server.txt
    docker inspect --format='{{{{json .RepoDigests}}}}' {{image_lit_static}}     | tr -d '[]" ' | cut -d',' -f1 | sed 's/.*@//' > .digest-lit-static.txt
    for f in .digest-lit-actions.txt .digest-lit-api-server.txt .digest-lit-static.txt; do
        [ -s "$f" ] || { echo "error: digest capture failed for $f"; exit 1; }
        printf "captured %s: %s\n" "$f" "$(cat $f)"
    done


# Deploy to Phala Cloud (requires: docker login, phala login).
# Builds, pushes, captures @sha256: digests, then substitutes them into the
# compose file (DR-1.1, DR-1.2). Override DOCKER_IMAGE (repo path) or
# DOCKER_TAG (to skip the build and reuse a prior push; digest files must exist).
# Use deploy to upgrade existing CVM; use deploy-new for first-time provisioning.
[group: 'deploy']
deploy: docker-push _check_phala
    #!/usr/bin/env sh
    set -eu
    DIGEST_LIT_ACTIONS=$(cat .digest-lit-actions.txt)
    DIGEST_LIT_API_SERVER=$(cat .digest-lit-api-server.txt)
    DIGEST_LIT_STATIC=$(cat .digest-lit-static.txt)
    [ -n "$DIGEST_LIT_ACTIONS" ]    || { echo "error: lit-actions digest missing; run: just docker-build"; exit 1; }
    [ -n "$DIGEST_LIT_API_SERVER" ] || { echo "error: lit-api-server digest missing; run: just docker-build"; exit 1; }
    [ -n "$DIGEST_LIT_STATIC" ]     || { echo "error: lit-static digest missing; run: just docker-build"; exit 1; }
    sed \
        -e "s|\${DOCKER_IMAGE_LIT_ACTIONS}|{{image_base}}-lit-actions@${DIGEST_LIT_ACTIONS}|g" \
        -e "s|\${DOCKER_IMAGE_LIT_API_SERVER}|{{image_base}}-lit-api-server@${DIGEST_LIT_API_SERVER}|g" \
        -e "s|\${DOCKER_IMAGE_LIT_STATIC}|{{image_base}}-lit-static@${DIGEST_LIT_STATIC}|g" \
        docker-compose.phala.yml > docker-compose.deploy.yml
    cat docker-compose.deploy.yml
    phala deploy -c docker-compose.deploy.yml --cvm-id {{app_name}} --instance-type {{instance_type}}

# Run locally with Docker Compose (no Phala Cloud)
[group: 'deploy']
docker-run-local: docker-build
    DOCKER_IMAGE_LIT_ACTIONS={{image_lit_actions}} \
    DOCKER_IMAGE_LIT_API_SERVER={{image_lit_api_server}} \
    DOCKER_IMAGE_LIT_STATIC={{image_lit_static}} \
    docker compose -f docker-compose.phala.yml up -d

[private]
_check_docker:
    #!/usr/bin/env sh
    set -eu
    command -v docker >/dev/null 2>&1 || { echo "error: docker not found. Install from https://docs.docker.com/get-docker/"; exit 1; }

[private]
_check_phala:
    #!/usr/bin/env sh
    set -eu
    command -v phala >/dev/null 2>&1 || { echo "error: phala not found. Run: just setup"; exit 1; }

[group: 'debug']
ssh:
    phala ssh
# Run k6 load tests. Default: smoke. Pass test names to run others.
# Usage:
#   just test                    # runs smoke
#   just test integration        # runs integration suite
#   just test sample             # runs k6-script.sample.ts
#   just test ecdsa-sign         # runs lit-action-ecdsa-sign.spec.ts
#   just test smoke integration  # runs both
#   BASE_URL=.../core/v1 just test
[group: 'test']
test *names='smoke':
    #!/usr/bin/env sh
    set -eu
    command -v k6 >/dev/null 2>&1 || { echo "error: k6 not found. Install from https://grafana.com/docs/k6/latest/set-up/install-k6/"; exit 1; }
    for t in {{names}}; do
        case "$t" in
            smoke)       k6 run k6/smoke.spec.ts ;;
            integration) k6 run k6/integration.spec.ts ;;
            ecdsa-sign)  k6 run k6/lit-action-ecdsa-sign.spec.ts ;;
            sample)      k6 run k6/k6-script.sample.ts ;;
            *) echo "error: unknown test '$t'. Available: smoke, integration, ecdsa-sign, sample"; exit 1 ;;
        esac
    done

# Build the dstack simulator from source (cached in /tmp/dstack).
# Skips the build if the binary already exists; re-run manually to update.
[group: 'sim']
sim-build:
    #!/usr/bin/env sh
    set -eu
    DSTACK_DIR=/tmp/dstack
    if [ ! -d "$DSTACK_DIR" ]; then
        echo "Cloning Dstack-TEE/dstack..."
        git clone --depth 1 https://github.com/Dstack-TEE/dstack.git "$DSTACK_DIR"
    fi
    echo "Building dstack-guest-agent (this takes a few minutes)..."
    cd "$DSTACK_DIR/sdk/simulator" && bash build.sh

# Build the dstack-verifier from source (cached in /tmp/dstack).
[group: 'sim']
verifier-build: sim-build
    #!/usr/bin/env sh
    set -eu
    echo "Building dstack-verifier..."
    cargo build \
        --manifest-path=/tmp/dstack/verifier/Cargo.toml \
        --bin dstack-verifier

# Start the dstack simulator in a unique temp directory so multiple runner
# instances on the same host can coexist without socket conflicts.
# Records the temp dir and PID in /tmp/dstack-sim.state; run `just sim-stop` to clean up.
[group: 'sim']
sim-start: sim-build
    #!/usr/bin/env sh
    set -eu
    SIM_SRC=/tmp/dstack/sdk/simulator
    STATE=/tmp/dstack-sim.state

    # Stop and clean up any previous instance recorded in the state file.
    if [ -f "$STATE" ]; then
        OLD_DIR=$(sed -n '1p' "$STATE")
        OLD_PID=$(sed -n '2p' "$STATE")
        kill "$OLD_PID" 2>/dev/null || true
        rm -rf "$OLD_DIR"
        rm -f "$STATE"
    fi

    # Create a fresh temp dir; the simulator creates sockets relative to its CWD,
    # so running from a unique dir isolates sockets per instance.
    SIM_TMP=$(mktemp -d /tmp/dstack-sim-XXXXXX)
    SIM_SOCK="$SIM_TMP/dstack.sock"

    # Copy the simulator's data files (referenced by relative paths in dstack.toml).
    cp "$SIM_SRC/appkeys.json" "$SIM_SRC/app-compose.json" \
       "$SIM_SRC/sys-config.json" "$SIM_SRC/attestation.bin" \
       "$SIM_SRC/dstack.toml" "$SIM_TMP/"

    echo "Starting dstack simulator in $SIM_TMP..."
    sh -c "cd '$SIM_TMP' && '$SIM_SRC/dstack-simulator'" >> "$SIM_TMP/dstack-simulator.log" 2>&1 &
    SIM_PID=$!

    printf '%s\n%s\n' "$SIM_TMP" "$SIM_PID" > "$STATE"

    for i in $(seq 1 15); do
        [ -S "$SIM_SOCK" ] && break
        printf "  waiting for dstack.sock (%d/15)...\n" "$i"
        sleep 1
    done
    [ -S "$SIM_SOCK" ] || { echo "error: dstack.sock never appeared"; cat "$SIM_TMP/dstack-simulator.log"; exit 1; }
    echo "Simulator ready at $SIM_SOCK (log: $SIM_TMP/dstack-simulator.log). Run: just sim-stop"
    printf '  DSTACK_SOCKET=%s\n' "$SIM_SOCK"

# Stop the dstack simulator and remove its temp directory.
[group: 'sim']
sim-stop:
    #!/usr/bin/env sh
    STATE=/tmp/dstack-sim.state
    if [ -f "$STATE" ]; then
        OLD_DIR=$(sed -n '1p' "$STATE")
        OLD_PID=$(sed -n '2p' "$STATE")
        kill "$OLD_PID" 2>/dev/null || true
        rm -rf "$OLD_DIR"
        rm -f "$STATE"
        echo "Simulator stopped and temp dir removed."
    else
        pkill -x dstack-simulator 2>/dev/null || true
        echo "Simulator stopped (no state file; sent pkill)."
    fi

# Build the dstack simulator (if needed), run phala attestation tests, then tear down.
# Each run gets its own temp dir so parallel invocations don't conflict.
[group: 'sim']
sim-test: sim-build
    #!/usr/bin/env sh
    set -eu
    SIM_SRC=/tmp/dstack/sdk/simulator
    SIM_TMP=$(mktemp -d /tmp/dstack-sim-XXXXXX)
    SIM_SOCK="$SIM_TMP/dstack.sock"
    PROJECT_ROOT="$(pwd)"

    # Copy simulator data files to the isolated temp dir.
    cp "$SIM_SRC/appkeys.json" "$SIM_SRC/app-compose.json" \
       "$SIM_SRC/sys-config.json" "$SIM_SRC/attestation.bin" \
       "$SIM_SRC/dstack.toml" "$SIM_TMP/"

    echo "Starting dstack simulator in $SIM_TMP..."
    sh -c "cd '$SIM_TMP' && '$SIM_SRC/dstack-simulator'" >> "$SIM_TMP/dstack-simulator.log" 2>&1 &
    SIM_PID=$!

    for i in $(seq 1 15); do
        [ -S "$SIM_SOCK" ] && break
        printf "  waiting for dstack.sock (%d/15)...\n" "$i"
        sleep 1
    done
    [ -S "$SIM_SOCK" ] || {
        echo "error: dstack.sock never appeared"
        cat "$SIM_TMP/dstack-simulator.log"
        kill "$SIM_PID" 2>/dev/null || true
        rm -rf "$SIM_TMP"
        exit 1
    }

    DSTACK_SOCKET="$SIM_SOCK" \
        cargo test --manifest-path="$PROJECT_ROOT/lit-api-server/Cargo.toml" \
            --features phala \
            -- phala::v1::dstack::tests --nocapture
    STATUS=$?

    kill "$SIM_PID" 2>/dev/null || true
    rm -rf "$SIM_TMP"
    exit "$STATUS"

# Get a live quote from the simulator, POST it to dstack-verifier, and assert quote_verified=true.
# Builds both the simulator and verifier if needed.
# Note: is_valid will be false for the simulator — its attestation.bin uses a synthetic OS image
# hash that is not published on download.dstack.org. This is expected; quote_verified=true is the
# meaningful assertion (it confirms the full get_quote() → verify pipeline is wired correctly).
[group: 'sim']
sim-verify: sim-build verifier-build
    #!/usr/bin/env sh
    set -eu
    SIM_SRC=/tmp/dstack/sdk/simulator
    SIM_TMP=$(mktemp -d /tmp/dstack-sim-XXXXXX)
    SIM_SOCK="$SIM_TMP/dstack.sock"
    VERIFIER_BIN=/tmp/dstack/target/debug/dstack-verifier
    VERIFIER_CFG=/tmp/dstack/verifier/dstack-verifier.toml
    VERIFIER_PORT=${DSTACK_VERIFIER_PORT:-28080}

    # Copy simulator data files.
    cp "$SIM_SRC/appkeys.json" "$SIM_SRC/app-compose.json" \
       "$SIM_SRC/sys-config.json" "$SIM_SRC/attestation.bin" \
       "$SIM_SRC/dstack.toml" "$SIM_TMP/"

    # Start simulator.
    echo "Starting dstack simulator in $SIM_TMP..."
    sh -c "cd '$SIM_TMP' && '$SIM_SRC/dstack-simulator'" >> "$SIM_TMP/dstack-simulator.log" 2>&1 &
    SIM_PID=$!
    for i in $(seq 1 15); do
        [ -S "$SIM_SOCK" ] && break
        printf "  waiting for dstack.sock (%d/15)...\n" "$i"
        sleep 1
    done
    [ -S "$SIM_SOCK" ] || {
        echo "error: dstack.sock never appeared"
        cat "$SIM_TMP/dstack-simulator.log"
        kill "$SIM_PID" 2>/dev/null || true
        rm -rf "$SIM_TMP"
        exit 1
    }

    # Start verifier.
    echo "Starting dstack-verifier on port $VERIFIER_PORT..."
    DSTACK_VERIFIER_PORT="$VERIFIER_PORT" "$VERIFIER_BIN" -c "$VERIFIER_CFG" \
        >> "$SIM_TMP/verifier.log" 2>&1 &
    VERIFIER_PID=$!
    for i in $(seq 1 15); do
        curl -sf "http://localhost:$VERIFIER_PORT/health" > /dev/null 2>&1 && break
        printf "  waiting for verifier (%d/15)...\n" "$i"
        sleep 1
    done
    curl -sf "http://localhost:$VERIFIER_PORT/health" > /dev/null 2>&1 || {
        echo "error: dstack-verifier did not start"
        cat "$SIM_TMP/verifier.log"
        kill "$SIM_PID" "$VERIFIER_PID" 2>/dev/null || true
        rm -rf "$SIM_TMP"
        exit 1
    }

    # Get a live quote from the simulator.
    echo "Getting quote from simulator..."
    QUOTE_RESP=$(curl -s --unix-socket "$SIM_SOCK" \
        http://localhost/GetQuote \
        -X POST \
        -H 'Content-Type: application/json' \
        -d '{"report_data":"0x"}')

    # Build the verifier request: strip 0x prefix from quote, add required attestation field.
    # (attestation must be explicitly null — the serde_human_bytes codec requires the key present)
    VERIFY_BODY=$(printf '%s' "$QUOTE_RESP" | python3 -c \
        'import sys,json; d=json.load(sys.stdin); q=d["quote"]; q=q[2:] if q.startswith("0x") else q; print(json.dumps({"quote":q,"event_log":d["event_log"],"vm_config":d["vm_config"],"attestation":None}))')

    # POST to verifier and parse response.
    echo "Verifying attestation with dstack-verifier..."
    VERIFY_RESP=$(printf '%s' "$VERIFY_BODY" | curl -s -X POST \
        "http://localhost:$VERIFIER_PORT/verify" \
        -H 'Content-Type: application/json' \
        -d @-)

    printf '%s' "$VERIFY_RESP" | python3 -c \
        'import sys,json; d=json.load(sys.stdin); det=d["details"]; [print(k+":",v) for k,v in [("is_valid",d["is_valid"]),("quote_verified",det["quote_verified"]),("event_log_verified",det["event_log_verified"]),("os_image_hash_verified",det["os_image_hash_verified"])]]; r=d.get("reason",""); r and print("reason:",r[:200])'

    # Extract quote_verified for assertion.
    QUOTE_OK=$(printf '%s' "$VERIFY_RESP" | python3 -c \
        'import sys,json; v=json.load(sys.stdin)["details"]["quote_verified"]; print("true" if v else "false")')

    # Cleanup.
    kill "$SIM_PID" "$VERIFIER_PID" 2>/dev/null || true
    rm -rf "$SIM_TMP"

    # quote_verified must be true — this validates the full attestation pipeline.
    # is_valid will be false for the simulator (synthetic OS image hash not on download.dstack.org).
    [ "$QUOTE_OK" = "true" ] || { echo "error: quote_verified=false — attestation pipeline is broken"; exit 1; }
    echo "Attestation pipeline verified: quote_verified=true."