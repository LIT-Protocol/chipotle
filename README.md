# lit-node-express

Monorepo for the Lit Protocol node stack: shared core libraries, the Lit Actions execution engine, and the HTTP API server.

---

## Projects

### 1. **lit-core**

Shared Rust workspace used by the API server and Lit Actions.

| Crate | Description |
|-------|-------------|
| **lit-core** | Core utilities, config, encoding, and platform support (e.g. `unix` for socket paths). |
| **lit-api-core** | API primitives: server/client helpers (Rocket, Hyper), error types, shared request/response types. |
| **lit-core-derive** | Procedural macros for lit-core. |
| **lit-logging** | Logging setup and filters. |
| **lit-observability** | OpenTelemetry metrics and tracing. |

**Location:** `lit-core/`

---

### 2. **lit-actions**

Runs Lit Action JavaScript in a sandboxed Deno environment. Exposes execution over gRPC; the API server connects as a client to run actions on demand.

| Crate | Description |
|-------|-------------|
| **lit-actions-server** | Deno-based JS runtime: executes code, handles op requests (sign, encrypt, fetch, etc.), enforces limits. |
| **lit-actions-grpc** | Protobuf definitions and gRPC client/server glue for the execute stream. |
| **lit-actions-ext** | Deno extension / bindings used by the server. |
| **cli** | CLI for running or testing actions. |
| **snapshot** / **tests** | Snapshots and test utilities. |

**Location:** `lit-actions/`

---

### 3. **lit-api-server**

Rocket HTTP API server that exposes Lit node operations and higher-level features. Serves static JavaScript SDKs and demo dApps.

- **Core API** (`/core/v1/`) — Handshake, get API key, mint PKP, sign with PKP, encrypt/decrypt, combine signature shares, Lit actions, ledger balance.
- **Transfer API** (`/transfer/v1/`) — Chain metadata (EVM/non-EVM, mainnet/testnet), balance by API key and chain, PKP native transfer.
- **Swaps API** (`/swaps/v1/`) — Swap intents: token list, quote lifecycle, attempt swap (QuoteStorage contract).
- **Static** — JS SDKs (`js_core_sdk.js`, `js_transfer_sdk.js`, `js_swaps_sdk.js`) and demo dApps (Solver, Swps PKP wallet).

**Dependencies:** Uses **lit-core** for config and utilities and **lit-actions-grpc** to talk to the Lit Actions server over gRPC (Unix socket or TCP).

**Location:** `lit-api-server/`  
**Details:** See [lit-api-server/README.md](lit-api-server/README.md).

---

## Dependency overview

```
lit-api-server  ──► lit-core (lit-core, lit-api-core, etc.)
                ──► lit-actions-grpc

lit-actions    ──► lit-core (lit-core, lit-api-core, lit-logging, lit-observability)
```

Build and run the API server from the repo root (ensure Lit Actions server is available on the configured socket if using actions):

```bash
cd lit-api-server && cargo run
```

By default the API is at `http://localhost:8000`; static files and dApps are served from `/`.

---

## Phala CVM deployment

A GitHub Actions workflow deploys both `lit-api-server` and `lit-actions` binaries to the smallest available Phala CVM (`tdx.small`):

- **Workflow:** `.github/workflows/deploy-phala.yml` — triggers on push to `main` or manual dispatch
- **Docker:** `Dockerfile.phala` — multi-stage build producing both binaries
- **Compose:** `docker-compose.phala.yml` — two services sharing a Unix socket for gRPC

**Required secrets:** `PHALA_CLOUD_API_KEY`, `DOCKER_REGISTRY_USERNAME`, `DOCKER_REGISTRY_PASSWORD`, `DOCKER_IMAGE`, `PHALA_APP_NAME`

See **[docs/deployment.md](docs/deployment.md)** for full documentation and architecture diagrams.

**Orchestration:** Development uses the simulator; released builds use either Phala Cloud (`pcloud`) or DeRoT on Base (`derot`) via Cargo feature flags.