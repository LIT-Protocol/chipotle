#!/usr/bin/env bash
# Print the pinned dstack-verifier image from .github/dstack-verifier/Dockerfile (#113).
#
# Fails unless that file has exactly one FROM line naming Phala's GHCR image by a
# stable x.y.z tag and a sha256 digest, so CI never falls back to a mutable tag.
set -euo pipefail

pin_file="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/.github/dstack-verifier/Dockerfile"

refs=$(sed -n -E 's/^FROM[[:space:]]+([^[:space:]]+)[[:space:]]*$/\1/p' "$pin_file")
count=$(printf '%s' "$refs" | grep -c . || true)
if [ "$count" -ne 1 ]; then
  echo "expected exactly one FROM line in $pin_file, found $count" >&2
  exit 1
fi

if ! printf '%s\n' "$refs" | grep -Eq '^ghcr\.io/dstack-tee/dstack-verifier:[0-9]+\.[0-9]+\.[0-9]+@sha256:[0-9a-f]{64}$'; then
  echo "dstack-verifier must be pinned as ghcr.io/dstack-tee/dstack-verifier:<x.y.z>@sha256:<digest> in $pin_file, got: $refs" >&2
  exit 1
fi

printf '%s\n' "$refs"
