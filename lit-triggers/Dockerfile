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

COPY --from=builder /build/lit-triggers/target/release/lit-triggers /app/lit-triggers
COPY SKILL.md ./SKILL.md
COPY static ./static
COPY migrations ./migrations

ENV ROCKET_ADDRESS=0.0.0.0
EXPOSE 8000

ENTRYPOINT ["/app/lit-triggers"]
