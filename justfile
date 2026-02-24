# Conventions: https://github.com/casey/just

image_base := env('DOCKER_IMAGE', 'litptcl/lit-node-express')
# Unique UUID tag per deploy (override with DOCKER_TAG to pin a specific build)
image_tag := env('DOCKER_TAG', `uuidgen | tr '[:upper:]' '[:lower:]' | tr -d '\n'`)
image := image_base + ':' + image_tag
app_name := env('PHALA_APP_NAME', 'lit-api-server')
instance_type := env('PHALA_INSTANCE_TYPE', 'tdx.small')

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

# Build Docker image for Phala deployment (release mode, linux/amd64 for Phala CVM)
[group: 'deploy']
docker-build: _check_docker
    #!/usr/bin/env sh
    set -eu
    docker build --platform linux/amd64 -f Dockerfile.phala -t {{image}} .
    docker push {{image}}

[group: 'deploy']
docker-push: docker-build


# Deploy to Phala Cloud (requires: docker login, phala login)
# Builds with unique UUID tag, pushes to registry, deploys that tagged image.
# Override DOCKER_IMAGE (repo path) or DOCKER_TAG (to pin a specific build).
# Use deploy to upgrade existing CVM; use deploy-new for first-time provisioning.
[group: 'deploy']
deploy: docker-push _check_phala
    #!/usr/bin/env sh
    set -eu
    sed "s|\${DOCKER_IMAGE}|{{image}}|g" docker-compose.phala.yml > docker-compose.deploy.yml
    phala deploy -c docker-compose.deploy.yml --cvm-id {{app_name}} --instance-type {{instance_type}}

# Run locally with Docker Compose (no Phala Cloud)
[group: 'deploy']
docker-run-local: docker-build
    DOCKER_IMAGE={{image}} docker compose -f docker-compose.deploy.yml up -d

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
#   just test              # runs smoke
#   just test sample       # runs k6-script.sample.ts
#   just test smoke sample # runs both
#   BASE_URL=.../core/v1 just test
[group: 'test']
test *names='smoke':
    #!/usr/bin/env sh
    set -eu
    command -v k6 >/dev/null 2>&1 || { echo "error: k6 not found. Install from https://grafana.com/docs/k6/latest/set-up/install-k6/"; exit 1; }
    for t in {{names}}; do
        case "$t" in
            smoke)  k6 run k6/smoke.spec.ts ;;
            sample) k6 run k6/k6-script.sample.ts ;;
            *) echo "error: unknown test '$t'. Available: smoke, sample"; exit 1 ;;
        esac
    done