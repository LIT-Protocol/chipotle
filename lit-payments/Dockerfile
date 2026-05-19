# Dockerfile for lit-payments.
#
# Build context: the REPO ROOT (so `../lit-billing-core` is reachable).
# On Railway: Settings → Build → "Root Directory" should be unset (or `/`),
# and "Dockerfile Path" should be `lit-payments/Dockerfile`.

# ── Build stage ────────────────────────────────────────────────────────────
FROM rust:1.91-slim-bookworm AS builder

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy the crates this service depends on.
COPY lit-billing-core ./lit-billing-core
COPY lit-payments    ./lit-payments

WORKDIR /build/lit-payments
RUN cargo build --release --bin lit-payments

# ── Runtime stage ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/lit-payments/target/release/lit-payments /app/lit-payments
COPY lit-payments/static     ./static
COPY lit-payments/migrations ./migrations

# Railway provides $PORT; main.rs translates it to ROCKET_PORT at startup.
ENV RUST_LOG=info

ENTRYPOINT ["/app/lit-payments"]
