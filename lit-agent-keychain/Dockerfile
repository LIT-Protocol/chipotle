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
# eMudhra intermediate CA compromise (2026-10-20, CVE-2026-111943): the private keys of
# eight eMudhra intermediates are public. They chain to "emSign ECC Root CA - G3" and, through
# Sectigo cross-signs, to "AAA Certificate Services". Distrust both roots in the OS trust store
# whatever ca-certificates version apt resolves (Debian 20250419~deb12u2 drops the same two), and
# fail the build if either is still trusted: OpenSSL/native-tls and rustls-native-certs clients
# verify against this store. https://emudhra.we-have-a-problem.com/
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 && rm -rf /var/lib/apt/lists/* \
    && sed -i -E 's@^mozilla/(emSign_ECC_Root_CA_-_G3|Comodo_AAA_Services_root)\.crt$@!&@' /etc/ca-certificates.conf \
    && update-ca-certificates --fresh \
    && openssl crl2pkcs7 -nocrl -certfile /etc/ssl/certs/ca-certificates.crt | openssl pkcs7 -print_certs -noout > /tmp/ca-subjects \
    && grep -q 'ISRG Root X1' /tmp/ca-subjects \
    && ! grep -E 'emSign ECC Root CA - G3|AAA Certificate Services' /tmp/ca-subjects \
    && ! find /etc/ssl/certs -lname '*emSign_ECC_Root_CA_-_G3*' -o -lname '*Comodo_AAA_Services_root*' | grep . \
    && rm /tmp/ca-subjects \
    && useradd --system --uid 10001 keychain
COPY --from=server /build/lit-agent-keychain/target/release/lit-agent-keychain /app/lit-agent-keychain
COPY --from=server /build/lit-agent-keychain/target/release/keychain-plan /app/keychain-plan
COPY --from=client /build/lit-agent-keychain/web/dist /app/web/dist
USER keychain
ENV ROCKET_ADDRESS=0.0.0.0
EXPOSE 8000
ENTRYPOINT ["/app/lit-agent-keychain"]
