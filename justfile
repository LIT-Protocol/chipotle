# Conventions: https://github.com/casey/just

image := env('DOCKER_IMAGE', 'litptcl/lit-node-express:latest')
app_name := env('PHALA_APP_NAME', 'lit-api-server')
instance_type := env('PHALA_INSTANCE_TYPE', 'tdx.small')

# List available recipes (default when invoked with no args)
default:
    @just --list

[group: 'build']
actions:
    cargo build --manifest-path=lit-actions/Cargo.toml --bin lit_actions

[group: 'build']
api-server:
    cargo build --manifest-path=lit-api-server/Cargo.toml --bin lit-api-server

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

# Build Docker image for Phala deployment (linux/amd64 for Phala CVM)
[group: 'deploy']
docker-build: _check_docker
    docker build --platform linux/amd64 -f Dockerfile.phala -t {{image}} .

[group: 'deploy']
docker-push: docker-build
    #!/usr/bin/env sh
    set -eu
    docker push {{image}}


# Deploy to Phala Cloud (requires: docker login, phala login)
# Default pushes to litptcl/lit-node-express. Override with DOCKER_IMAGE.
# Use deploy to upgrade existing CVM; use deploy-new for first-time provisioning.
[group: 'deploy']
deploy: docker-push _check_phala
    #!/usr/bin/env sh
    set -eu
    sed "s|\${DOCKER_IMAGE}|{{image}}|g" docker-compose.phala.yml > docker-compose.deploy.yml
    phala deploy -c docker-compose.deploy.yml --cvm-id {{app_name}} --instance-type {{instance_type}}

# First-time deploy (provision new CVM). Use deploy for subsequent upgrades.
[group: 'deploy']
deploy-new: docker-push _check_phala
    #!/usr/bin/env sh
    set -eu
    sed "s|\${DOCKER_IMAGE}|{{image}}|g" docker-compose.phala.yml > docker-compose.deploy.yml
    phala deploy -c docker-compose.deploy.yml -n {{app_name}} --instance-type {{instance_type}}

# Run locally with Docker Compose (no Phala Cloud)
[group: 'deploy']
run-local: docker-build
    DOCKER_IMAGE={{image}} docker compose -f docker-compose.phala.yml up -d

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
