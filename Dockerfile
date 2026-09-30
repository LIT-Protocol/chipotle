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

# Build RELEASE binaries (no workspace root, so build each crate by manifest path).
# Release builds disable `debug_assertions`, which compiles out the
# cfg(debug_assertions)-gated dev auth bypass (LIT_DEV_WALLET_BYPASS) in
# lit-billing-core — a debug build reaching production would re-enable it. Never
# ship a debug build (CPL-379 L11).
WORKDIR /app/lit-actions
RUN cargo build --release --bin lit_actions
WORKDIR /app/lit-api-server
RUN cargo build --release --bin lit-api-server

# Runtime stage: minimal image with binaries and entrypoint
FROM ubuntu:24.04 AS runtime
WORKDIR /app

# Runtime deps: OpenSSL and CA certs for TLS / HTTPS
RUN apt-get update -y && apt-get install -y --no-install-recommends \
    openssl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy built binaries into PATH
COPY --from=builder /app/lit-api-server/target/release/lit-api-server /usr/local/bin/
COPY --from=builder /app/lit-actions/target/release/lit_actions /usr/local/bin/


# Copy static assets (served by lit-api-server)
COPY --from=builder /app/lit-api-server/static /app/lit-api-server/static/

# Copy configuration file (main branch uses NodeConfig.main.toml; default: next)
ARG NODE_CONFIG=NodeConfig.next.toml
COPY lit-api-server/${NODE_CONFIG} /app/NodeConfig.toml

# Copy and set entrypoint script (starts lit_actions in background, then lit-api-server)
COPY DockerEntryPoint.sh /usr/local/bin/DockerEntryPoint.sh
RUN chmod +x /usr/local/bin/DockerEntryPoint.sh

# Run as a non-root system user (defense in depth; CPL-379 L11). The binaries in
# /usr/local/bin stay root-owned but world-executable; /app is chowned so the app
# can read its config/static assets and write any runtime state under it.
RUN groupadd --system lit \
    && useradd --system --gid lit --home-dir /app --no-create-home --shell /usr/sbin/nologin lit \
    && chown -R lit:lit /app
USER lit

ENTRYPOINT ["/usr/local/bin/DockerEntryPoint.sh"]
