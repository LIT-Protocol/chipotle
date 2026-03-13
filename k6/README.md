# k6 Load Tests for lit-api-server

Auto-generated from the OpenAPI spec using [@grafana/openapi-to-k6](https://github.com/grafana/openapi-to-k6).

## Prerequisites

- [k6](https://grafana.com/docs/k6/latest/set-up/install-k6/)
- [Node.js](https://nodejs.org/) (for regenerating from OpenAPI)

## Regenerating from OpenAPI

```bash
npm install -g @grafana/openapi-to-k6
openapi-to-k6 https://f8fce543471dc9f5f5643aa217422398c36e5edc-8001.dstack-base-prod5.phala.network/openapi.json \ ./k6

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
k6 run k6/smoke.spec.ts
```

**Full sample script** (all endpoints; uses placeholder params—customize for real tests):

```bash
k6 run k6/k6-script.sample.ts
```

**Custom base URL:**

```bash
BASE_URL=http://localhost:8000 k6 run k6/smoke.spec.ts
```

## Files

- `litApiServer.ts` – Generated TypeScript client (do not edit manually)
- `k6-script.sample.ts` – Sample script exercising all endpoints
- `smoke.spec.ts` – Minimal smoke test for `get_node_chain_config`
