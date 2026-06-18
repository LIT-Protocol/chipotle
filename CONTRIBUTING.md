# Contributing to Lit Chipotle

This page is the one place that lists everything you need installed and how the
repo fits together. The [README](README.md) covers the product; this covers the
development workflow.

## Prerequisites

Everything the full local stack and test suites use:

| Tool | Needed for | Install |
|------|-----------|---------|
| Rust toolchain (edition 2024) | building everything | [rustup.rs](https://rustup.rs/) |
| [just](https://github.com/casey/just) | task runner (`just --list`) | `brew install just` |
| [Foundry](https://getfoundry.sh/) (anvil, forge, cast) | local chain + contract deploys | `curl -L https://foundry.paradigm.xyz \| bash && foundryup` |
| [dstack simulator](https://github.com/Dstack-TEE/dstack) | TEE socket for local dev | `git clone https://github.com/Dstack-TEE/dstack.git ~/GitHub/dstack && cd ~/GitHub/dstack/sdk/simulator && bash build.sh` (or `just sim-build`) |
| [static-web-server](https://static-web-server.net/) | serving the dashboard locally | `brew install static-web-server` |
| Node.js 20+ and pnpm | e2e tests, examples, k6 client regeneration | [nodejs.org](https://nodejs.org/) |
| protobuf-compiler | lit-actions gRPC build | `brew install protobuf` |
| jq | deploy tooling | `brew install jq` |
| [k6](https://grafana.com/docs/k6/latest/set-up/install-k6/) | integration/load tests (optional) | `brew install k6` |
| Docker | Jaeger tracing, container builds (optional) | [docker.com](https://www.docker.com/) |

## Repo map

| Path | What it is |
|------|------------|
| `lit-api-server/` | The REST API (Rust/Rocket). Routes under `/core/v1/`. |
| `lit-actions/` | Sandboxed JS runtime (Deno embedded in Rust), gRPC over a Unix socket. |
| `lit-core/` | Shared crates: config/env/logging, API framework, observability. |
| `lit-billing-core/` | Shared Stripe primitives (used by lit-api-server and lit-payments). |
| `lit-payments/` | Ops credit-grant portal + LITKEY crypto payments (Railway). |
| `lit-triggers/` | Webhook/cron/chain-event trigger service (Railway). |
| `lit-static/` | Dashboard + JS Core SDK (`core_sdk.js`) + contract ABIs. |
| `docs/` | **Source of truth for developer.litprotocol.com** (Mintlify). |
| `architectureDocs/` | Internal architecture and deployment docs. |
| `examples/` | Standalone end-to-end examples (each with its own README + .env.example). |
| `e2e/` | Playwright end-to-end suites (see `e2e/README.md`). |
| `k6/` | Load + correctness tests, client auto-generated from the OpenAPI spec. |

## Common workflows

**Run the full local stack** (Anvil + dstack sim + contracts + api server + actions + dashboard):

```bash
./local_test.sh
```

**Build / lint / unit-test:**

```bash
just build          # builds lit-actions and lit-api-server
just fmt            # cargo fmt across the Rust crates
just clippy         # cargo clippy
cd lit-api-server && cargo test
cd lit-actions && cargo test
```

**Integration tests (k6), against a running stack:**

```bash
just test                       # smoke by default
just test smoke integration     # specific suites
BASE_URL=https://host/core/v1 just test smoke   # remote target
```

**End-to-end browser tests:** see [e2e/README.md](e2e/README.md) (`make install && make up && make test`).

**Docs site:** edit `docs/**/*.mdx` (Mintlify). Preview with `npx mint dev` from
`docs/`. Navigation lives in `docs/docs.json`.

**After changing API routes or request/response models**, regenerate the
OpenAPI spec and the k6 client — CI (`k6-client-check`) fails otherwise:

```bash
cd lit-api-server && cargo run --bin openapi_spec > ../spec.json && cd ..
npx @grafana/openapi-to-k6 spec.json ./k6
```

**Deploy** (Phala Cloud): see `justfile.deploy` (`just deploy <app>`) and
[architectureDocs/deployment/deployment.md](architectureDocs/deployment/deployment.md).

## PR conventions

- Branch from `main`; PRs run fmt/clippy/test plus `k6-client-check` (spec drift).
- Keep `docs/` in sync with behavior changes — the docs site ships from this repo.
- Security issues: do **not** open a public issue; see [SECURITY.md](SECURITY.md).
