# Agent Context: OpenTelemetry Collector

This folder configures the **OpenTelemetry Collector** that ships traces/metrics/logs
for the stack. It is config + entrypoint only — not a Rust crate and not application code.

## Purpose
The OpenTelemetry Collector config for the Phala sidecar. Receives OTLP (gRPC) from
`lit-api-server` and `lit-actions` on `0.0.0.0:4317`, scrapes CVM host metrics,
enriches spans/metrics/logs with lit-protocol resource attributes, and exports them
to GCP Cloud Monitoring / Trace / Logging via the `googlecloud` exporter. Auth comes
from `GOOGLE_APPLICATION_CREDENTIALS`, which `entrypoint.sh` writes from the
`GCP_SERVICE_ACCOUNT_JSON` Phala secret.

## Stack & Tooling
- `config.yaml`: collector pipeline (receivers, processors, exporters).
- `entrypoint.sh`: container startup.

## Coding Rules
- Validate YAML before committing; a malformed config crashes the collector on boot.
- Never hardcode secrets, tokens, or endpoints — reference environment variables.
- Keep receiver/exporter changes in sync with what the services actually emit and where telemetry is shipped.

## Definition of Done
1. `config.yaml` is valid YAML and parses with the collector version in use.
2. The collector starts cleanly via `entrypoint.sh` with no pipeline errors.
3. No secrets committed; all sensitive values come from the environment.
