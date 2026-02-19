# Architecture Simplification: lit-observability

The goal is to transition from a complex UDS/Proxy-based telemetry architecture to a simplified, sidecar-oriented OTLP stack.

## Target Architecture
- **Transport**: Standard HTTP/gRPC (OTLP) to a local sidecar.
- **Collector**: Stock `otel-collector` (external to binary).
- **Crate Role**: Convenience wrapper for Lit-specific OTLP datatypes and privacy filtering.

## To-Do List

### Phase 1: Core Library Refactor (lit-observability) [COMPLETED]
- [x] **Gut Transport Layer**: Remove UDS, manual retries, and hyper/tonic connectors in `net.rs`.
- [x] **Standardize Exporter**: Implement `telemetry.endpoint` config for OTLP.
- [x] **Remove Legacy Features**: Delete `proxy-collector` feature and related interceptors.
- [x] **Clean lib.rs**: Simplify `create_providers` to remove redundant builder logic.
- [x] **Dependency Audit**: Clean up `Cargo.toml` and remove unused crates (hyper-util, etc).

### Phase 2: Consumer Integration (lit-api-server) [COMPLETED]
- [x] **Update Initializer**: Update `main.rs` to reflect the simplified `create_providers` signature.
- [x] **Config Update**: Ensure `Rocket.toml` or default config includes the new `telemetry.endpoint`.
- [x] **Remove UDS Config**: Delete any references to `logging_service_grpc_socket_path` in the API server.

### Phase 3: Infrastructure & Deployment [COMPLETED]
- [x] **Update Dockerfile**: Add `otel-collector` sidecar configuration.
- [x] **Fly.io Adjustment**: sidecar runs in container via entrypoint.sh.
- [x] **Validation**: Run the API server and verify traces hit the sidecar via OTLP/gRPC.

### Phase 4: Final Cleanup [COMPLETED]
- [x] **Verify Privacy Filters**: Ensure `PrivacyModeLayer` still functions correctly without the legacy transport.
- [x] **Final PR Submission**: Submit the consolidated changes.
- [x] **Documentation**: Architecture documented in README.md with mermaid diagram.
