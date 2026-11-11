# Relay and action latency diagnostics

The staging soak workflow also runs `k6/loadtest/relay.spec.ts` sequentially on
its existing runner against the **same `BASE_URL`**, then validates the results
with `report.mjs`. This supplements the existing soak: its workloads, thresholds,
and committed baselines are unchanged. Production runs do not execute this new
account-creating benchmark.

Five workloads distinguish shared request overhead from wallet-specific work:

| Workload | Action |
| --- | --- |
| `hello` | Constant return value, no wallet operation |
| `action_sign` | `getLitActionPrivateKey()` and message signing |
| `pkp_sign` | `getPrivateKey({pkpId})` and message signing |
| `relay_wallet` | Pinned Relay bundle, wallet derivation |
| `relay_sign` | Same bundle, fabricated attestation and digest signing |

Each operation runs twice to warm up and ten times for measurement. One client,
one VU, sequential requests, no measured retries. Timing surrounds the generated
k6 client call through JSON parsing, using its host's millisecond clock. Signature
verification and setup are outside the timer. These are network-inclusive samples,
not a load/capacity test; with ten samples, nearest-rank p95 is the maximum.

Setup explicitly requires `K6_ENV=staging` and a Stripe `pk_test_` configuration
before creating an account. It uses the existing k6 account/funding helper, mints
a separate PKP (never exports the account master wallet), creates scoped wallet
permissions, and polls readiness before timing. It creates a real account/wallet
and uses a $10 sandbox top-up. The group is removed afterward, including after
partial permission setup; the account and wallet remain. Setup/teardown failures
fail k6. No blockchain transfers are signed by the measured actions.

The offline verifier requires exactly 60 ordered, distinct request IDs, valid
result envelopes, expected results, stable addresses, and signatures recovering
to the expected address. It compares PKP signatures to the minted PKP and Relay
signatures to that run's derived Relay address. This is **not** persistent wallet
continuity across deployments; the separate chipotle-testing saved-wallet suite
provides that coverage. Errors, truncated output, and missing telemetry IDs fail
validation. Raw action logs, keys, and response bodies are not persisted in the
JSON artifact. No new performance baseline is silently established or updated.

## Run and inspect

From the repository root, with Node 22+ and k6 installed:

```sh
(cd performance/relay && pnpm install --frozen-lockfile)
node --test performance/relay/*.test.mjs
K6_ENV=staging BASE_URL=https://test.chipotle.litprotocol.com/core/v1 \
  K6_CORRELATION_ID=relay-measurement-UNIQUE \
  k6 run --log-format raw --console-output relay-samples.log k6/loadtest/relay.spec.ts
node performance/relay/report.mjs relay-samples.log relay-performance.json
```

Always require **both** k6 and the verifier to succeed. A complete sample set
cannot prove teardown succeeded; CI independently gates on the k6 exit code.
CI attaches `relay-performance.json` alongside the soak report and adds a table
to the job summary. It includes the actual endpoint, observed deployed version,
fixture/spec hashes, public PKP ID, raw timings, and `X-Request-Id` for each sample.
Do not compare a direct Phala endpoint to the public routing domain as if they
were the same network path. Do not compare fresh CI wallets to saved-wallet
benchmarks without recording the different setup and runner.

## Exact server trace correlation (optional, read-only)

Use an identity with `cloudtrace.traces.list` / `cloudtrace.traces.get` access.
The existing observability reader key works; alternatively use Google Application
Default Credentials. Do not put a reader key in the repository or an artifact.
After telemetry has ingested:

```sh
GCP_PROJECT=chipotle-next \
  CHIPOTLE_OBSERVABILITY_CREDENTIALS="$HOME/.config/chipotle-observability/reader.json" \
  node performance/relay/traces.mjs relay-performance.json relay-traces.json
```

The command does not execute actions or update baselines. It reads the benchmark
window, matches the server-issued request ID to the exact `http_request` span,
and records the GCP trace ID, server duration, and `client duration - server
duration`. That remainder includes routing, proxy, client, and uninstrumented
work; **it is not pure internet RTT**. Nested worker/private-key/account-lookup
spans must descend from the matched HTTP span. Child and parent durations overlap;
parallel account reads must not be added together as critical-path latency.

Missing/ambiguous spans, invalid durations, and query truncation produce an
explicit incomplete result and exit 2; API/auth failures exit 1. Re-run enrichment
after ingestion if needed; the original measurements stay untouched. CI does not
require or provision GCP credentials. Timing IDs permit later investigation without
inferring requests from their order, or publishing raw trace attributes.

## Fixture provenance and optimization boundary

`fixtures/relay.js` and `fixtures/sign-params.json` are copied from the public
[LIT-Protocol/chipotle-testing](https://github.com/LIT-Protocol/chipotle-testing)
`fixtures/relay/ethereum-vm.sign.bundled.js` and
`ethereum-vm.sign-jsparams.json`. The bundle is byte-identical to the original
saved-wallet benchmark (SHA-256
`0dad112787b745f58f26cfdda51493ab1ac855a08c709fdb7e9148df391f4a54`), including its
pinned CDN imports. The attestation uses fabricated test identities and does not
submit a withdrawal. Fixture changes require deliberate review and invalidate
comparisons with earlier action hashes.

The new account-admin/billing-wallet check on `getPrivateKey()` is preserved.
It currently performs two parallel fresh contract reads; ordinary action-derived
signing does not use this path. Caching these reads across requests can invalidate
the protection after an admin/billing-wallet change. Any future optimization must
preserve fresh, fail-closed checks and pass the relevant security tests. A combined
fresh contract view or RPC batching deserves separate measurement and contract/RPC
compatibility review; removing a check or introducing a stale TTL is not a latency
fix in this change.
