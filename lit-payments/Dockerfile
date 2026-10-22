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

# Copy the crates this service depends on.
COPY lit-billing-core ./lit-billing-core
COPY lit-payments    ./lit-payments

WORKDIR /build/lit-payments
RUN cargo build --release --bin lit-payments

# ── Runtime stage ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# eMudhra intermediate CA compromise (2026-10-20, CVE-2026-111943): the private keys of
# eight eMudhra intermediates are public. They chain to "emSign ECC Root CA - G3" and, through
# Sectigo cross-signs, to "AAA Certificate Services". Distrust both roots in the OS trust store
# whatever ca-certificates version apt resolves (Debian 20250419~deb12u2 drops the same two), and
# fail the build if either is still trusted: OpenSSL/native-tls and rustls-native-certs clients
# verify against this store. https://emudhra.we-have-a-problem.com/
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/* \
 && sed -i -E 's@^mozilla/(emSign_ECC_Root_CA_-_G3|Comodo_AAA_Services_root)\.crt$@!&@' /etc/ca-certificates.conf \
 && update-ca-certificates --fresh \
 && openssl crl2pkcs7 -nocrl -certfile /etc/ssl/certs/ca-certificates.crt | openssl pkcs7 -print_certs -noout > /tmp/ca-subjects \
 && grep -q 'ISRG Root X1' /tmp/ca-subjects \
 && ! grep -E 'emSign ECC Root CA - G3|AAA Certificate Services' /tmp/ca-subjects \
 && ! find /etc/ssl/certs -lname '*emSign_ECC_Root_CA_-_G3*' -o -lname '*Comodo_AAA_Services_root*' | grep . \
 && rm /tmp/ca-subjects

WORKDIR /app

COPY --from=builder /build/lit-payments/target/release/lit-payments /app/lit-payments
COPY lit-payments/static     ./static
COPY lit-payments/migrations ./migrations

# Fly.io (and most container hosts) provide $PORT; main.rs translates it to
# ROCKET_PORT at startup.
ENV RUST_LOG=info

ENTRYPOINT ["/app/lit-payments"]
