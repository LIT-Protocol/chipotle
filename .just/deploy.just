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

[group: 'debug']
ssh:
    phala ssh

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
