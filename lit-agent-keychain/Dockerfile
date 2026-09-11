# Build from repository root: docker build -f lit-agent-keychain/Dockerfile .
# The context includes the pinned Rust IPFS hashing fix in vendor/.
FROM node:22-bookworm-slim AS client
RUN apt-get update && apt-get install -y --no-install-recommends python3 make g++ && rm -rf /var/lib/apt/lists/*
WORKDIR /build/lit-agent-keychain
COPY lit-agent-keychain/package*.json ./
RUN npm ci
COPY lit-agent-keychain/ ./
ARG VITE_LIT_API_URL=https://api.chipotle.litprotocol.com
ARG VITE_WALLETCONNECT_PROJECT_ID
ENV VITE_LIT_API_URL=$VITE_LIT_API_URL
ENV VITE_WALLETCONNECT_PROJECT_ID=$VITE_WALLETCONNECT_PROJECT_ID
RUN npm run build

FROM rust:1.91-bookworm AS server
WORKDIR /build/lit-agent-keychain
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
COPY vendor/ipfs-hasher /build/vendor/ipfs-hasher
COPY lit-agent-keychain/ ./
COPY --from=client /build/lit-agent-keychain/generated ./generated
RUN cargo build --locked --release --bins

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 && rm -rf /var/lib/apt/lists/* && useradd --system --uid 10001 keychain
COPY --from=server /build/lit-agent-keychain/target/release/lit-agent-keychain /app/lit-agent-keychain
COPY --from=server /build/lit-agent-keychain/target/release/keychain-plan /app/keychain-plan
COPY --from=client /build/lit-agent-keychain/web/dist /app/web/dist
USER keychain
ENV ROCKET_ADDRESS=0.0.0.0
EXPOSE 8000
ENTRYPOINT ["/app/lit-agent-keychain"]
