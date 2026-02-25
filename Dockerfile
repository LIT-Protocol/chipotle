# Build stage: compile lit-actions and lit-api-server with Rust toolchain
FROM rust:1.88.0-slim-bookworm AS builder
WORKDIR /app

# Install project Rust toolchain (edition 2024)
# RUN rustup toolchain install 1.88.0 && rustup default 1.88.0

# Install build dependencies (OpenSSL, protobuf, pkg-config for native deps)
RUN apt-get update -y && apt-get install -y --no-install-recommends \
    build-essential \
    openssl \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    ca-certificates \
    curl \
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy full source (lit-api-server, lit-actions, lit-core)
COPY . .

# Build release binaries (no workspace root, so build each crate by manifest path)
# RUN cargo build --release --manifest-path lit-api-server/Cargo.toml --bin lit-api-server
# RUN cargo build --release --manifest-path lit-actions/Cargo.toml --bin lit_actions
WORKDIR /app/lit-actions
RUN cargo build --bin lit_actions
WORKDIR /app/lit-api-server
RUN cargo build --bin lit-api-server

# Runtime stage: minimal image with binaries and entrypoint
FROM ubuntu:24.04 AS runtime
WORKDIR /app

# Runtime deps: OpenSSL and CA certs for TLS / HTTPS
RUN apt-get update -y && apt-get install -y --no-install-recommends \
    openssl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy built binaries into PATH
COPY --from=builder /app/lit-api-server/target/debug/lit-api-server /usr/local/bin/
COPY --from=builder /app/lit-actions/target/debug/lit_actions /usr/local/bin/


# Copy static assets (served by lit-api-server)
COPY --from=builder /app/lit-api-server/static /app/lit-api-server/static/

# Copy demo configuration file
COPY NodeConfig.demo.toml /app/lit-api-server/NodeConfig.toml
COPY NodeConfig.demo.toml /usr/local/bin/NodeConfig.toml
COPY NodeConfig.demo.toml /app/NodeConfig.toml

# Copy and set entrypoint script (starts lit_actions in background, then lit-api-server)
COPY DockerEntryPoint.sh /usr/local/bin/DockerEntryPoint.sh
RUN chmod +x /usr/local/bin/DockerEntryPoint.sh

ENTRYPOINT ["/usr/local/bin/DockerEntryPoint.sh"]
