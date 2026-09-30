#!/usr/bin/env bash
# Publish @lit-protocol/keychain. Bumps the version only when the built package
# differs from what the registry serves. See scripts/publish-sdk.mjs.
#   ./publish.sh                 # detect changes, bump patch if needed, publish
#   ./publish.sh --bump minor    # bump minor instead of patch when changed
#   ./publish.sh --dry-run       # show the decision without publishing or committing
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
exec node scripts/publish-sdk.mjs "$@"
