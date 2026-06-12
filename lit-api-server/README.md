# lit-api-server

The Chipotle REST API: a Rust/Rocket server exposing account, key, group, and
Lit Action execution endpoints under `/core/v1/`. Runs inside a TEE (Phala
dstack) in production; against the dstack simulator locally.

> This README previously documented `/transfer/v1/` and `/swaps/v1/` APIs and
> their demo dApps — those were removed from the server. The API surface today
> is `/core/v1/` plus `/attestation`, `/health`, and `/dstack/v1/`.

## Run it

From the repo root, `./local_test.sh` boots the full stack (Anvil chain,
dstack simulator, contracts, this server, lit-actions, dashboard). To run just
this server against an already-running simulator + chain:

```bash
cargo run                        # dev (dstack socket via DSTACK_SOCKET)
cargo run --features production  # prod behavior (hardcoded socket)
```

The server listens on `0.0.0.0:8000` (see `Rocket.toml`). The dashboard is
served separately from `lit-static/` (`http://localhost:8080` under
`local_test.sh`).

## Configuration

**NodeConfig.toml** (required in the working directory at startup; see
`NodeConfig.sample.toml`, plus the `.next` / `.main` / `.prod` variants baked
into images):

| Field | Description |
|-------|-------------|
| `chain.name` | Chain hosting the AccountConfig contract (e.g. `anvil`, `base`) |
| `chain.contract_address` | Deployed AccountConfig diamond address |

Other files: `Rocket.toml` (listen address/port), `rpc-config.yaml` (RPC
endpoints per chain), `log_levels.toml` (per-module log levels).

Environment variables (all optional) are documented in the repo root
[`.env.example`](../.env.example): `RUST_LOG`, `LIT_TELEMETRY_ENDPOINT`,
`DSTACK_SOCKET`, `BASE_CHAIN_RPC`, `STRIPE_SECRET_KEY`,
`STRIPE_PUBLISHABLE_KEY`, `STARTER_CREDITS_CENTS`, `CPU_OVERLOAD_MULTIPLIER`,
`CPU_PSI_THRESHOLD`.

## Module map

| Path | Purpose |
|------|---------|
| `src/main.rs` | Startup, restart loop (on-chain `ServerTriggered`), Rocket build, catcher/fairing registration |
| `src/core/v1/endpoints/` | Route handlers (account mgmt, actions, billing, configuration) |
| `src/core/v1/guards/` | API-key extraction, billing enforcement, CPU load shedding |
| `src/core/v1/catchers.rs` | JSON error bodies (`{error, message, fix, docs_url}`) |
| `src/core/` | Business logic behind the endpoints |
| `src/accounts/` | AccountConfig contract reads/writes, signer pool, chain-config cache |
| `src/actions/` | gRPC client to the lit-actions runtime + Deno op handlers |
| `src/stripe.rs` | Credit ledger on Stripe customer balances (checks, charges, starter credits) |
| `src/dstack/` | TEE attestation + key derivation via the dstack socket |
| `blockchain/` | Contract tooling (see below) |

## Billing model

With Stripe configured, management writes cost a flat $0.01 (checked in the
guard, settled by a response fairing only after success) and Lit Actions bill
$0.01/second during execution. Without Stripe env vars, billing is disabled and
nothing is charged. Error semantics: invalid key → 401, insufficient credits →
402 (body states amount needed and how to fund), billing infra down → 503. See
the [Errors reference](https://developer.litprotocol.com/management/errors).

## OpenAPI

The spec is generated from the route definitions:

```bash
cargo run --bin openapi_spec > ../spec.json
npx @grafana/openapi-to-k6 ../spec.json ../k6   # keep the k6 client in sync (CI-enforced)
```

Swagger UI: `/core/v1/swagger-ui`. Raw spec: `/core/v1/openapi.json`.

## Blockchain tooling (`blockchain/`)

| Path | Description |
|------|-------------|
| `lit_node_express/` | AccountConfig contracts. `make generate` compiles Solidity and regenerates the checked-in Alloy bindings via `forge bind`. |
| `rust_generator_and_deployer/` | `contract_deployer` CLI: deploy/update the AccountConfig diamond (`--action=deploy\|update\|propose-update`, `--network=anvil\|yellowstone\|base-sepolia\|base`, `--abifolder=…`). |

From the repo root: `just contracts-generate` and `just contracts-deploy`.

## Tests

```bash
cargo test       # unit tests
just test        # k6 smoke against a running stack (from the repo root)
```
