#!/usr/bin/env bash
# Wrapper for the stripe_report CLI (CPL-269).
#
# Usage:
#   STRIPE_SECRET_KEY=sk_live_...  \
#   STRIPE_PUBLISHABLE_KEY=pk_live_...  \
#   ./scripts/stripe_report.sh [--days N] [--out PATH] [--csv-only]
#
# Defaults: --days 30, --out ./stripe-report (writes .csv and .html).
#
# Requires `cargo` on PATH. Runs the release build for speed.

set -euo pipefail

if [[ -z "${STRIPE_SECRET_KEY:-}" || -z "${STRIPE_PUBLISHABLE_KEY:-}" ]]; then
  echo "error: STRIPE_SECRET_KEY and STRIPE_PUBLISHABLE_KEY must be set in the environment." >&2
  echo "       export them before running, e.g.:" >&2
  echo "         export STRIPE_SECRET_KEY=sk_live_..." >&2
  echo "         export STRIPE_PUBLISHABLE_KEY=pk_live_..." >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT/lit-api-server"

exec cargo run --release --bin stripe_report -- "$@"
