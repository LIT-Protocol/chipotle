FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --bin lit-node-simple-api

# We do not need the Rust toolchain to run the binary!
# FROM debian:bookworm-slim AS runtime // we need modern ssl support
FROM ubuntu:24.04 AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/lit-node-simple-api /usr/local/bin
COPY live_testnet.toml /app
COPY log_levels.toml /app
COPY rpc-config.yaml /app
copy static /app/static

ENTRYPOINT ["/usr/local/bin/lit-node-simple-api"]

RUN apt-get update -y && apt-get install -y openssl 

# Update package lists and install ca-certificates
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*