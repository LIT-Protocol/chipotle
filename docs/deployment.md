# Deployment

This document describes how to deploy the Lit node stack (`lit-api-server` and `lit-actions`) to Phala Cloud using the smallest available CVM instance.

## Overview

The deployment uses:

- **GitHub Actions** — CI/CD workflow triggered on push to `main` or manual dispatch
- **Docker** — Multi-stage build producing both binaries in a single image
- **Docker Compose** — Two services sharing a Unix socket for gRPC communication
- **Phala Cloud** — Confidential Virtual Machine (CVM) with TEE, instance type `tdx.small`

## Architecture

```mermaid
flowchart TB
    subgraph CVM["Phala CVM (tdx.small)"]
        subgraph containers["Docker Compose"]
            API["lit-api-server<br/>:8000"]
            Actions["lit-actions<br/>gRPC server"]
            eRPC["eRPC (future)<br/>EVM RPC proxy<br/>:4000"]
            Vol["lit-socket volume<br/>/tmp/lit_actions.sock"]
        end
        Dstack["/var/run/dstack.sock<br/>TEE"]
    end

    Client["HTTP Client"] -->|"POST /core/v1/lit_action"| API
    API -->|"Unix socket"| Actions
    API -.->|"shared volume"| Vol
    Actions -.->|"creates socket"| Vol
    API -.->|"chain RPCs (future)"| eRPC
    API --> Dstack
    Actions --> Dstack

    subgraph CI["GitHub Actions"]
        Checkout["Checkout"] --> Build["Build image"]
        Build --> Push["Push to registry"]
        Push --> Deploy["phala deploy"]
    end

    Push -.->|"image:tag"| containers

    subgraph future["Future (production)"]
        DeRoT["DeRoT on Base<br/>orchestrate upgrades"]
    end

    Deploy -.->|"govern (future)"| DeRoT

    style eRPC stroke-dasharray: 5 5
    style DeRoT stroke-dasharray: 5 5
```

## Files

| File | Purpose |
|------|---------|
| `.github/workflows/deploy-phala.yml` | GitHub Actions workflow |
| `Dockerfile.phala` | Multi-stage build for both binaries |
| `docker-compose.phala.yml` | Service definitions and shared socket volume |
| `.dockerignore` | Excludes build artifacts from Docker context |

## Build

The `Dockerfile.phala` produces a single image containing:

- `lit_actions` — Lit Actions gRPC server (Deno-based JS runtime)
- `lit-api-server` — Rocket HTTP API server (built with `phala` feature for attestation)

Both run as separate containers in the same CVM, communicating via a shared Unix socket at `/tmp/lit_actions.sock`.

### Phala attestation

When built with the `phala` feature, `lit-api-server` exposes `GET /phala/v1/verify`, which returns a TDX attestation quote from the dstack socket. Callers can verify the CVM is running in genuine Intel TDX hardware.

## Required Secrets

Configure these in **Settings → Secrets and variables → Actions**:

| Secret | Description |
|--------|-------------|
| `PHALA_CLOUD_API_KEY` | From [Phala Cloud Dashboard](https://cloud.phala.network/dashboard) → Avatar → API Tokens |
| `DOCKER_REGISTRY_USERNAME` | Docker Hub or GHCR username |
| `DOCKER_REGISTRY_PASSWORD` | Docker Hub access token or GHCR PAT |
| `DOCKER_IMAGE` | Full image path, e.g. `docker.io/username/lit-node-express` or `ghcr.io/owner/lit-node-express` |
| `PHALA_APP_NAME` | CVM name, e.g. `lit-api-server` |

## Workflow Steps

1. **Checkout** — Clone the repository
2. **Log in to registry** — Authenticate with Docker Hub or GHCR
3. **Build and push** — Build the image, tag with a unique UUID, and push
4. **Prepare compose** — Substitute `${DOCKER_IMAGE}` with the built image tag
5. **Deploy** — Run `phala deploy` with `--instance-type tdx.small`

## Manual Deployment

Using [just](https://github.com/casey/just) (recommended):

```bash
just setup       # optional: install Phala CLI (requires npm)
just deploy      # builds with UUID tag, pushes to registry, and deploys that image
```

**Prerequisites:** Log in to Docker Hub (`docker login`) and ensure you have push access to `litptcl/lit-node-express`. The deploy command updates an existing CVM by name; for first-time deploy when no CVM exists, use `just deploy-new`.

Override with `DOCKER_IMAGE` (repo path without tag) or `DOCKER_TAG` (to pin a specific build):

```bash
DOCKER_IMAGE=ghcr.io/owner/lit-node-express just deploy
DOCKER_TAG=abc123-def456 just deploy   # deploy a specific tag
```

Or run the commands directly (after `docker login` and `phala login`):

```bash
# Build with UUID tag, push, and deploy
TAG=$(uuidgen | tr '[:upper:]' '[:lower:]')
docker build -f Dockerfile.phala -t litptcl/lit-node-express:$TAG .
docker push litptcl/lit-node-express:$TAG
sed "s|\${DOCKER_IMAGE}|litptcl/lit-node-express:$TAG|g" docker-compose.phala.yml > docker-compose.deploy.yml
phala deploy -c docker-compose.deploy.yml -n lit-api-server --instance-type tdx.small
```

Environment variables: `DOCKER_IMAGE` (default: `litptcl/lit-node-express`, repo path without tag), `DOCKER_TAG` (default: auto-generated UUID), `PHALA_APP_NAME` (default: `lit-api-server`), `PHALA_INSTANCE_TYPE` (default: `tdx.small`).

## Instance Type

The workflow uses `tdx.small`, the smallest available Phala CVM plan. For custom sizing:

```bash
phala deploy --vcpu 1 --memory 2048MB --disk-size 40GB ...
```

## Integration Testing

Use [Grafana k6](https://grafana.com/docs/k6/latest/) to run integration tests against the deployed API. The script exercises user flows: get chain config, get Lit Action IPFS ID, and execute a simple Lit Action that returns "Hello World!".

**Prerequisites:** Install k6 ([install guide](https://grafana.com/docs/k6/latest/set-up/install-k6/)).

```bash
just k6-test
```

Or with a custom base URL:

```bash
BASE_URL=https://your-instance.phala.network just k6-test
```

To test `lit_action` with an existing account (skips `new_account` and `add_group`):

```bash
LIT_API_KEY=your-base64-api-key just k6-test
```

The full flow creates a new account and group when `LIT_API_KEY` is not set; this requires the AccountConfig contract to be deployed and configured on the chain (e.g. Base Sepolia).

## Current Limitations

- **No autoscaling** — Phala CVM autoscaling is not currently configured; the deployment runs a fixed instance.
- **No chain RPCs** — Chain RPC endpoints are not provided for this deployment; configure external RPCs as needed.

## Future Integrations (Tentative)

**[eRPC](https://github.com/erpc/erpc)** — fault-tolerant EVM RPC proxy and re-org aware caching solution. A future, optional third-party service that could be added as a Docker Compose service to provide chain RPC endpoints for this deployment. eRPC offers retries, circuit breakers, failovers, rate limiting, and a unified dashboard. Integration is not planned or implemented; this is a placeholder for potential future work.

## Orchestration: Development vs Released

| Environment | Orchestration |
|-------------|---------------|
| **Development** | Simulator — local TEE simulator for development. |
| **Released** | Either Phala Cloud or [DeRoT](https://docs.phala.com/dstack/design-documents/decentralized-root-of-trust) on Base, selected via Cargo feature flags: `pcloud` (Phala Cloud) or `derot` (on-chain governance on Base). |
