# k6 Load Tests for lit-api-server

Auto-generated from the OpenAPI spec using [@grafana/openapi-to-k6](https://github.com/grafana/openapi-to-k6).

## Prerequisites

- [k6](https://grafana.com/docs/k6/latest/set-up/install-k6/)
- [Node.js](https://nodejs.org/) (for regenerating from OpenAPI)

## Regenerating from OpenAPI

```bash
npm install -g @grafana/openapi-to-k6
openapi-to-k6 https://api.dev.litprotocol.com/openapi.json \ ./k6

# Note: The OpenAPI spec paths omit the /core/v1 prefix. Set BASE_URL with /core/v1
# when running against the Phala deployment, e.g. BASE_URL=.../core/v1
```

Or use a local OpenAPI file:

```bash
openapi-to-k6 --include-sample-script ./path/to/openapi.json ./k6
```

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
- `LitActionCode/` – Shared Lit Action code (Hello World, Encrypt, Decrypt) for tests
