# Dockerfile for lit-payments.
#
# Build context: the REPO ROOT (so the sibling `lit-billing-core/` crate is
# reachable via the COPY commands below).
#
# Railway service config lives at `lit-payments/railway.json`; keep the Railway
# service build context at the repo root so these COPY commands can see sibling
# crates.

# ── Build stage ────────────────────────────────────────────────────────────
FROM rust:1.91-slim-bookworm AS builder

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache-bust BEFORE the COPY so the build re-reads the committed source (incl. the
# migrations/ dir). A bust placed after the COPY only re-runs the compile against
# whatever the cached COPY layer brought in — which let a stale draft migration
# persist in the embedded `sqlx::migrate!` set. Bump on any migrations/ change.
ARG MIGRATIONS_REV=20260629000001-r3
RUN echo "source rev: ${MIGRATIONS_REV}"

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

# Cache-bust before copying migrations into the runtime image. The app now loads
# migrations from this directory at RUNTIME (see db::run_migrations), so these
# files — not anything embedded in the binary — are the source of truth. Keep
# this bust in lockstep with the builder one above. Bump on any migrations/ change.
ARG MIGRATIONS_REV=20260629000001-r3
RUN echo "runtime migrations rev: ${MIGRATIONS_REV}"

COPY --from=builder /build/lit-payments/target/release/lit-payments /app/lit-payments
COPY lit-payments/static     ./static
COPY lit-payments/migrations ./migrations

# Fly.io (and most container hosts) provide $PORT; main.rs translates it to
# ROCKET_PORT at startup.
ENV RUST_LOG=info

ENTRYPOINT ["/app/lit-payments"]
