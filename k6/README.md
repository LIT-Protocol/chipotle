# k6 Load Tests for lit-api-server

Auto-generated from the OpenAPI spec using [@grafana/openapi-to-k6](https://github.com/grafana/openapi-to-k6).

## Prerequisites

- [k6](https://grafana.com/docs/k6/latest/set-up/install-k6/)
- [Node.js](https://nodejs.org/) (for regenerating from OpenAPI)

## Client generation

CI generates `litApiServer.ts` from the checked-out Rust sources before smoke,
correctness, and load tests. The `k6-client-check` job verifies generation and test
imports and uploads the generated client as an artifact. API changes no longer
require a separate generated-file commit to pass CI.

The committed client is a convenience for local runs. Refresh it from the repo root:

```bash
bash k6/generate-client.sh
```

This needs Rust and protobuf-compiler as well as Node.js. The script pins the
client generator and formatter, and only replaces `litApiServer.ts`.
To generate from an existing spec instead of compiling Rust:

```bash
bash k6/generate-client.sh ./path/to/openapi.json
# Optional second argument selects a separate output directory:
bash k6/generate-client.sh https://api.chipotle.litprotocol.com/core/v1/openapi.json /tmp/k6-client
```

### OpenAPI spec drift

The deployment `openapi-spec-check` compares clients generated from the checked-out
sources and the deployed `/openapi.json`. It deliberately keeps an independent
source baseline: generating both sides from the deployed API would hide drift.
If it fails, check that the deployed revision matches the workflow checkout and
that rollout has completed. The diff includes documentation changes as well as
API types and routes; regenerating the committed client cannot repair a deployment
mismatch. This check retains its existing advisory (`continue-on-error`) behavior.

The OpenAPI paths omit `/core/v1`; include that prefix in `BASE_URL` when targeting
Phala deployments.

## Running Tests

**Smoke test** (public endpoint, no auth):

```bash
just -f k6/justfile smoke-local
# or
k6 run k6/smoke.spec.ts
```

**Full sample script** (all endpoints; uses placeholder params—customize for real tests):

```bash
k6 run k6/k6-script.sample.ts
```

**Custom base URL:**

```bash
BASE_URL=http://localhost:8000 k6 run k6/smoke.spec.ts

## Pre-created accounts for tests

Most k6 tests create new accounts and usage keys, which can add noise to the
system and slow down test startup. You can instead pre-create a pool of
accounts once and have all tests reuse them.

1. **Seed accounts to a local file** (from repo root):

```bash
# Default: create 40 accounts and write them to k6/data/accounts.json
just -f k6/justfile accounts-seed

# Custom number of accounts
ACCOUNTS_COUNT=80 just -f k6/justfile accounts-seed
```

This runs `k6/accounts.seed.spec.ts`, which creates accounts and usage API keys
and writes them as JSON to `k6/data/accounts.json`.

2. **Run tests using the pre-created pool:**

All existing k6 tests now read a random subset of accounts from this JSON file
instead of creating new ones in `setup()`. As long as `k6/data/accounts.json`
exists (or a custom file pointed to by `ACCOUNTS_FILE`), tests like `smoke`,
`spike`, `soak`, `breakpoint`, and the correctness specs will reuse these
accounts.

You can override the accounts file path with:

```bash
ACCOUNTS_FILE=./data/my-accounts.json k6 run k6/loadtest/soak.spec.ts
```
```

## Files

- `litApiServer.ts` – Generated TypeScript client (do not edit manually)
- `k6-script.sample.ts` – Sample script exercising all endpoints
- `smoke.spec.ts` – Minimal smoke test for `get_node_chain_config`
- `LitActionCode/` – Shared Lit Action code (Hello World, Encrypt, Decrypt) for
  tests
