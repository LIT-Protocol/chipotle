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

# Build release binaries (no workspace root, so build each crate by manifest path).
# CPL-379 L11: build --release. Debug builds ship with `debug_assertions` on,
# which in production would enable the cfg(debug_assertions)-gated dev auth
# bypass (LIT_DEV_WALLET_BYPASS) among other debug-only paths.
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

# CPL-379 L11: run as a non-root user (defense-in-depth — a container escape
# from the API/actions process lands as an unprivileged user). Rocket binds a
# non-privileged port (>=1024), so no root is needed at runtime.
RUN useradd --system --create-home --uid 10001 lituser \
    && chown -R lituser:lituser /app
USER lituser

ENTRYPOINT ["/usr/local/bin/DockerEntryPoint.sh"]
