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

ChainSecured (sovereign) accounts split this: admin writes are wallet-signed
contract calls that never reach the metered HTTP endpoints, so the
`BilledManagementApiKey` guard never fires and the $0.01 management charge is
*not* applied (the user pays Base gas instead). Lit Action execution still
flows through `BilledLitActionApiKey` and bills $0.01/second exactly as in API
mode. See [API Mode vs ChainSecured Mode](https://developer.litprotocol.com/management/account_modes#how-billing-splits-in-chainsecured-mode).

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

## Opt-in platform-aware attestation

`GET /attestation/v3?nonce=<64 lowercase hex characters>` returns fresh dstack
attestation evidence on supported Intel TDX and AMD SEV-SNP guests. This is an
additional evidence endpoint, not an automatic SDK upgrade or a verification
verdict. `/attestation`, `/attestation/v2`, `/info`, SDK defaults, and all existing
key derivation paths and purposes are unchanged.

The caller generates a cryptographically random 32-byte nonce for each check.
The response is:

```json
{
  "version": 3,
  "nonce": "<the caller's 32-byte nonce, lowercase hex>",
  "report_data": "<64 bytes, hex>",
  "attestation": "<opaque dstack VersionedAttestation bytes, hex>"
}
```

On AMD SEV-SNP the response also has a `collateral` object when this instance
has it cached: `vcek` (the chip's VCEK certificate, DER as hex) and `ask` (AMD's
SEV intermediate, PEM), fetched once from AMD's Key Distribution Service. AMD's
service sends no CORS headers and rate limits by address, so browsers cannot
fetch these themselves. They are AMD-signed and a verifier must check them
against AMD root keys it pins itself; treat them as untrusted input. The field
is absent on other platforms and until the first fetch succeeds, in which case
clients that can reach AMD fetch them directly.

`report_data` is `keccak256(UTF8("chipotle-attestation-v3:") || nonce_bytes)`
followed by 32 zero bytes. This uses Keccak-256, not SHA3-256. The server submits
these 64 bytes to the local dstack `/Attest` RPC and returns its evidence bytes
unchanged. Chipotle's response version `3` is independent of the dstack envelope's
internal version. Consumers must use a compatible dstack VersionedAttestation
decoder, not the old raw-TDX-quote parser. No KMS-derived key is requested by v3;
existing `GetKey` calls remain on the legacy guest API.

The endpoint requires a guest agent supporting platform-adaptive `Attest` (the
prod16 migration targets dstack 0.6.1). The frozen v0 surface also includes
`Attest` on v0.5.11. The SDK now exposes `attest()`, but does not bound response
size or request duration. This endpoint uses a separate bounded HTTP client over the same Unix
socket; existing callers retain their SDK behavior.
Production still fixes the socket to `/var/run/dstack.sock`; development uses the
existing `DSTACK_SOCKET` override. There is no TCP fallback or HTTP redirect.
See the upstream [Attest contract](https://github.com/Dstack-TEE/dstack/blob/0e75b442ce29001f30ee1e0cbfd17094988846c6/docs/guest-api-v0.md#attest).

Success and error responses use `Cache-Control: no-store`; evidence is generated
for each valid, admitted request. Agent error details are logged server-side, not
returned to callers.
Missing/invalid nonces return 400. Only one v3 attestation request runs at a time
per API process; concurrent requests return 429 and should retry with backoff.
Agent errors, unsupported agents, malformed JSON, empty/non-hex evidence and
oversized responses return 503. The agent request has a 20-second timeout and a
2 MiB response limit.
There is no fallback to a cached quote or an older endpoint.

A new verifier must authenticate the complete platform evidence (including its
certificate chain, TCB and policy), recompute and check the expected report data
inside that verified evidence, and validate the measured app, compose and OS
identity against the intended governance policy. Merely comparing the JSON
`nonce` and `report_data` echoes is insufficient. This endpoint does not bind the
HTTPS connection to the CVM or attest a signing key; channel binding requires a
separate verification step. Unknown platforms and unsupported evidence versions
must fail closed. This PR does not supply that new SDK verifier.

The reference client verifier is `lit-static/attestation.js`, published as
`@lit-protocol/core-sdk/attestation`. Every deploy runs it against the new
instance (`lit-static/sdk/scripts/verify-live.mjs`).

**Compatibility boundary:** adding v3 does not make an AMD deployment compatible
with existing Intel-only clients. Keep the existing production origin on TDX
while those clients are supported. An AMD rollout needs an explicit opt-in origin,
a new verifier, live verification on that origin, and key-continuity validation
using the same app identity and derivation paths before users migrate. Do not
route legacy clients to AMD or disable their verification to make checks pass.

The unpublished `/attestation/unified` endpoint is removed, including its
`/dstack/v1` alias. Deployment verification now requests v3 with a fresh nonce and
checks the authenticated verifier report data against the locally computed
commitment. Deploy this API and its verification workflow together; older
servers lacking v3 fail verification rather than falling back to weaker evidence.

The v3 concurrency cap protects resources, not per-client fairness: an anonymous
caller can still occupy its slot. Per-client admission policy is deferred until
ingress client-IP trust is validated; do not disable verification on a 429 response.

## Running more than one instance at once

Every instance of the app derives the same keys. When more than one serves
traffic at the same time (one hostname per platform during a migration), set
these per instance. All are unset
by default, which is correct for a single serving instance.

| Variable | Purpose |
| --- | --- |
| `LIT_PAYER_SHARD` | `<index>/<count>`, for example `0/2`. The share of the payer pool this instance signs with: payer `n` belongs to shard `(n - 1) mod count`. Instances count nonces in memory, so two that share a payer reuse each other's nonces. Shard 0 alone registers and funds payers. A sharded pool never drains a payer dropped from the pool (another instance may be signing with it), so its gas stays until swept by hand. Startup fails if the shard owns no registered payer. |
| `LIT_PEER_BASE_URLS` | Comma-separated `https` origins of the other instances. After a top-up this instance tells them to drop the customer's cached balance. |
| `LIT_ORIGIN_SUNSET` | An HTTP-date, for example `Fri, 15 Jan 2027 00:00:00 GMT`. Adds a `Sunset` header to every response. |
| `LIT_ORIGIN_SUCCESSOR_URL` | With `LIT_ORIGIN_SUNSET`: adds `Link: <url>; rel="successor-version"`. |
