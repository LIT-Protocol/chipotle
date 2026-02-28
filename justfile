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
            sample)      k6 run k6/k6-script.sample.ts ;;
            *) echo "error: unknown test '$t'. Available: smoke, integration, sample"; exit 1 ;;
        esac
    done

# Build the dstack simulator from source (cached in /tmp/dstack).
# Skips the build if the binary already exists; re-run manually to update.
[group: 'test']
simulator-build:
    #!/usr/bin/env sh
    set -eu
    DSTACK_DIR=/tmp/dstack
    if [ ! -d "$DSTACK_DIR" ]; then
        echo "Cloning Dstack-TEE/dstack..."
        git clone --depth 1 https://github.com/Dstack-TEE/dstack.git "$DSTACK_DIR"
    fi
    if [ ! -x "$DSTACK_DIR/sdk/simulator/dstack-simulator" ]; then
        echo "Building dstack-guest-agent (this takes a few minutes)..."
        cd "$DSTACK_DIR/sdk/simulator" && bash build.sh
    else
        echo "Simulator already built: $DSTACK_DIR/sdk/simulator/dstack-simulator"
    fi

# Build the dstack simulator (if needed), run phala attestation tests against it, then stop it.
[group: 'test']
test-phala-sim: simulator-build
    #!/usr/bin/env sh
    set -eu
    SIM_DIR=/tmp/dstack/sdk/simulator
    SIM_SOCK="$SIM_DIR/dstack.sock"

    # Kill any stale simulator process and remove the old socket.
    pkill -f dstack-simulator 2>/dev/null || true
    rm -f "$SIM_SOCK"

    # Start simulator in background (cd into its directory so it finds its data files,
    # then immediately return to the project root for the cargo invocation below).
    PROJECT_ROOT="$(pwd)"
    echo "Starting dstack simulator..."
    (cd "$SIM_DIR" && ./dstack-simulator > /tmp/dstack-simulator.log 2>&1) &
    SIM_PID=$!

    # Wait up to 15 s for the socket to appear.
    i=1
    while [ $i -le 15 ]; do
        [ -S "$SIM_SOCK" ] && break
        printf "  waiting for dstack.sock (%d/15)...\n" "$i"
        sleep 1
        i=$((i + 1))
    done
    if [ ! -S "$SIM_SOCK" ]; then
        echo "error: dstack.sock never appeared. Simulator log:"
        cat /tmp/dstack-simulator.log
        kill "$SIM_PID" 2>/dev/null || true
        exit 1
    fi

    echo "Simulator ready (pid $SIM_PID, socket $SIM_SOCK)"

    # Run the phala dstack tests against the live simulator.
    DSTACK_SIMULATOR_ENDPOINT="$SIM_SOCK" \
        cargo test --manifest-path="$PROJECT_ROOT/lit-api-server/Cargo.toml" \
            --features phala \
            -- phala::v1::dstack::tests --nocapture
    STATUS=$?

    # Always stop the simulator.
    kill "$SIM_PID" 2>/dev/null || true
    exit "$STATUS"