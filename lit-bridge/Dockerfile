# Dockerfile for lit-bridge.
#
# Build context: the REPO ROOT (matches the other services' Railway config).
# lit-bridge has no path-dependency on sibling crates, so it only needs itself.

# ── Build stage ────────────────────────────────────────────────────────────
FROM rust:1.91-slim-bookworm AS builder

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY lit-bridge ./lit-bridge

WORKDIR /build/lit-bridge
RUN cargo build --release --bin lit-bridge

# ── Runtime stage ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/lit-bridge/target/release/lit-bridge /app/lit-bridge
COPY lit-bridge/static ./static

ENV RUST_LOG=info
ENTRYPOINT ["/app/lit-bridge"]
