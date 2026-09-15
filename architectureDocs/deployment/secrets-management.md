# Secrets Management

How runtime secrets reach the Lit Chipotle CVM, what the attestation does and does
**not** cover about them, and the policies that govern them.

> **Status:** the mechanism below is accurate to `docker-compose.phala.yml`. The
> policy items (who can set/read, rotation cadence, dev/prod separation) are marked
> **TODO/DECIDE** where not yet finalized.

## The important caveat: secrets are outside the attestation

Attestation proves *what code* the enclave runs (the compose hash in RTMR3) and *what
OS* it booted. It does **not** measure the values of injected environment variables.
Secrets are supplied to the CVM as **encrypted Phala CVM environment variables**,
resolved by Phala at container startup and decrypted inside the TEE.

Consequences a reviewer should understand:

- The compose hash pins the *names and wiring* of env vars (they appear in the
  digest-pinned `docker-compose`), but **not their values**. Changing a secret's
  value does not change the compose hash.
- Whoever controls the Phala account (the `PHALA_CLOUD_API_KEY`) controls what gets
  injected. This is why that key is a high-value target — see
  [incident-response.md](incident-response.md) Scenario 3.
- The secrets are protected *in transit and at rest* by Phala's encrypted-env
  mechanism and only decrypted inside the attested enclave; they are not exposed to
  the host. But their *provenance* is not part of the on-chain trust chain.

## Secrets injected today

From `docker-compose.phala.yml` (all set as encrypted Phala CVM env vars):

| Secret | Used by | Purpose | Blast radius if leaked |
|---|---|---|---|
| `BASE_CHAIN_RPC` | lit-api-server | Base RPC endpoint | Low (read-mostly endpoint) |
| `STRIPE_SECRET_KEY` | lit-api-server | Stripe billing (live in prod) | **High** — payments |
| `STRIPE_PUBLISHABLE_KEY` | lit-api-server | Stripe client key | Low (publishable) |
| `GCP_SERVICE_ACCOUNT_JSON` | otel-collector | Ship telemetry to GCP | Medium — scoped to telemetry project |
| `GCP_PROJECT_ID` | otel-collector | GCP target project | Low |
| `CERTBOT_AWS_ACCESS_KEY_ID` / `…SECRET_ACCESS_KEY` / `…ROLE_ARN` | dstack-ingress | Route53 DNS-01 for TLS | **High** — DNS control; scope to DNS-01 only |

## Policies

- **Least privilege per credential.** Each credential should be scoped to exactly its
  function (e.g. the Route53 IAM principal limited to DNS-01 TXT records on the cert
  domain; the GCP service account limited to telemetry ingestion). **TODO/DECIDE (still
  open, 2026-06-25):** the live IAM policy/role documents live in AWS/GCP and are not
  checked into this repo — only the credential *names* (`CERTBOT_AWS_*`,
  `GCP_SERVICE_ACCOUNT_JSON`) appear here. Capture the actual scopes in this doc.
- **Dev vs prod separation.** Partially confirmed (2026-06-25) from the deploy
  workflows: staging (`deploy-staging.yml`) uses Stripe **sandbox** keys and GCP project
  `chipotle-next`; prod (`deploy-prod-1-propose.yml`) uses **live** Stripe keys and GCP
  project `chipotle-prod`, deploying to separate CVMs. **TODO/DECIDE (still open):** both
  workflows reference the *same* `PHALA_CLOUD_API_KEY` secret, so a separate Phala
  account/project per environment is **not** confirmed — verify whether staging and prod
  use distinct Phala accounts (or GitHub environments scoping that secret) so no prod
  secret is reachable from a dev deploy.
- **Who can set/read.** Setting CVM secrets requires the Phala account credentials.
  **TODO/DECIDE:** name the owning role and how access is granted/revoked.
- **Rotation.** **TODO/DECIDE:** define rotation cadence per credential and the
  rotate-then-redeploy procedure (see incident runbook Scenario 5).

## Telemetry egress note

The `otel-collector` ships logs/metrics to GCP from inside the CVM — a data-plane
egress out of the TEE. Exported telemetry must contain **no secrets or request
plaintext** (only operational metrics and scrubbed logs). **Verified 2026-06-25:** the
collector pipelines (`otel-collector/config.yaml`) apply only `memory_limiter`,
`resource` (stamps `service.namespace`/`cloud.provider`), and `batch` — there is **no**
`redaction`/`filter`/`attributes` scrubbing processor. The no-secrets guarantee
therefore rests entirely on the application side not emitting secrets into spans/logs.
**TODO/DECIDE:** either add a collector-side redaction/allow-list processor or document
and enforce the source-side scrubbing contract.

## References

- `docker-compose.phala.yml` — the source of truth for injected env vars
- [deployment.md](deployment.md) — required CI secrets/variables
- [incident-response.md](incident-response.md) — secret-compromise response
- [Phala: CVM environment variables / secrets](https://docs.phala.com/)
