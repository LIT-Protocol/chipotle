# Dockerfile for lit-triggers.
# Build from the repository root with Dockerfile path `lit-triggers/Dockerfile`.

FROM rust:1.91-bookworm AS builder
WORKDIR /build

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY lit-triggers ./lit-triggers
WORKDIR /build/lit-triggers
RUN cargo build --release --bin lit-triggers

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/lit-triggers/target/release/lit-triggers /app/lit-triggers
COPY lit-triggers/static ./static
COPY lit-triggers/migrations ./migrations

ENV ROCKET_ADDRESS=0.0.0.0
EXPOSE 8000

ENTRYPOINT ["/app/lit-triggers"]
