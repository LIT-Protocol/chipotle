# lit-core

Shared Rust crates used by `lit-api-server` and `lit-actions`.

| Crate | Purpose |
|-------|---------|
| `lit-core` | Config/env loading, error types, logging setup, shared utils |
| `lit-core-derive` | Proc macros for the above |
| `lit-api-core` | API framework helpers: Rocket metrics fairings, server/client builders, TLS/compression |
| `lit-observability` | Tracing subscriber setup; OTLP trace/metric/log export behind the `otlp` feature |

Build/test from each crate directory with `cargo build` / `cargo test`, or via
`just build` at the repo root.
