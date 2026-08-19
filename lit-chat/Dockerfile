# Lit Chat CVM image: one image, two binaries (lit-chat + lit-chat-admin),
# selected per-container via `command:` in docker-compose.phala.yml.
# Build with the Docker build context set to this `lit-chat/` directory:
#   docker build -t lit-chat -f Dockerfile .
# Production builds pin the dstack socket and disable dev fallbacks:
#   --features production (used below).

FROM rust:1.91-bookworm AS builder
WORKDIR /build/lit-chat

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --locked --release --features production \
    --bin lit-chat --bin lit-chat-admin

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/lit-chat/target/release/lit-chat /app/lit-chat
COPY --from=builder /build/lit-chat/target/release/lit-chat-admin /app/lit-chat-admin
COPY static ./static
COPY admin-static ./admin-static
COPY migrations ./migrations

ENV ROCKET_ADDRESS=0.0.0.0
EXPOSE 8000 8100

ENTRYPOINT ["/app/lit-chat"]
