# Architecture Simplification: lit-observability

The goal is to transition from a complex UDS/Proxy-based telemetry architecture to a simplified, sidecar-oriented OTLP stack.

## Target Architecture
- **Transport**: Standard HTTP/gRPC (OTLP) to a local sidecar.
- **Collector**: Stock `otel-collector` (external to binary).
- **Crate Role**: Convenience wrapper for Lit-specific OTLP datatypes and privacy filtering.

## To-Do List

### Phase 1: Core Library Refactor (lit-observability) [IN PROGRESS]
- [x] **Gut Transport Layer**: Remove UDS, manual retries, and hyper/tonic connectors in `net.rs`.
- [x] **Standardize Exporter**: Implement `telemetry.endpoint` config for OTLP.
- [x] **Remove Legacy Features**: Delete `proxy-collector` feature and related interceptors.
- [x] **Clean lib.rs**: Simplify `create_providers` to remove redundant builder logic.
- [ ] **Dependency Audit**: Clean up `Cargo.toml` and remove unused crates (hyper-util, etc).

### Phase 2: Consumer Integration (lit-api-server)
- [ ] **Update Initializer**: Update `main.rs` to reflect the simplified `create_providers` signature.
- [ ] **Config Update**: Ensure `Rocket.toml` or default config includes the new `telemetry.endpoint`.
- [ ] **Remove UDS Config**: Delete any references to `logging_service_grpc_socket_path` in the API server.

### Phase 3: Infrastructure & Deployment
- [ ] **Update Dockerfile**: Add `otel-collector` sidecar configuration.
- [ ] **Fly.io Adjustment**: Update `fly.toml` to allow sidecar services if applicable.
- [ ] **Validation**: Run the API server and verify traces hit the sidecar via OTLP/gRPC.

### Phase 4: Final Cleanup
- [ ] **Verify Privacy Filters**: Ensure `PrivacyModeLayer` still functions correctly without the legacy transport.
- [ ] **Final PR Submission**: Submit the consolidated changes.
