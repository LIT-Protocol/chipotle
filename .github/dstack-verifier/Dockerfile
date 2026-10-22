# Pinned dstack-verifier image for the CI attestation checks (issue #113).
#
# This file is never built. scripts/dstack-verifier-image.sh reads the FROM line
# below, and verify-attestation.yml and phala-simulator.yml run exactly this
# digest, so a push to the registry cannot change what CI runs.
#
# Updates: Dependabot (.github/dependabot.yml) opens a PR here when Phala
# publishes a newer stable release. Pre-releases are skipped while this pin is a
# stable version. dstack-verifier-provenance.yml checks every change to this file
# against Phala's signed build provenance for the matching verifier-v<version> tag.
#
# Releases: https://github.com/Dstack-TEE/dstack/releases (tags verifier-v*)
FROM ghcr.io/dstack-tee/dstack-verifier:0.6.1@sha256:9f2c07a94efd3d8a1a89268e5faa35e91e3cb345b9c4ab859c86d35142f4f788
