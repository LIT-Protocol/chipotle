# Dockerfile for lit-triggers.
# Build with the Docker build context set to this `lit-triggers/` directory.
# On Railway, configure the service root directory to `lit-triggers` so this
# Dockerfile, railway.json, static assets, and migrations are all in-context.

FROM rust:1.91-bookworm AS builder
WORKDIR /build/lit-triggers

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --release --bin lit-triggers

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/lit-triggers/target/release/lit-triggers /app/lit-triggers
COPY SKILL.md ./SKILL.md
COPY static ./static
COPY migrations ./migrations

ENV ROCKET_ADDRESS=0.0.0.0
EXPOSE 8000

ENTRYPOINT ["/app/lit-triggers"]
