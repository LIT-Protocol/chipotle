#!/usr/bin/env bash
# 02-fetch-chipotle.sh — download (or update) the latest Chipotle source from
# GitHub. NEEDS THE INTERNET (this is part of the install phase).
#
# If a checkout already exists (CHIPOTLE_DIR or ~/GitHub/chipotle), it is updated
# with `git pull`. Otherwise it is cloned fresh.
#
# Env:
#   CHIPOTLE_DIR   Target checkout dir (default: ~/GitHub/chipotle)
#   CHIPOTLE_REPO  Clone URL (default: https://github.com/LIT-Protocol/chipotle.git)
#   CHIPOTLE_REF   Branch/tag/commit to check out (default: main)
#
# Usage:  bash local-runbook/scripts/02-fetch-chipotle.sh
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; source "$DIR/_common.sh"

REPO="${CHIPOTLE_REPO:-https://github.com/LIT-Protocol/chipotle.git}"
REF="${CHIPOTLE_REF:-main}"
TARGET="${CHIPOTLE_DIR:-$HOME/GitHub/chipotle}"

command -v git >/dev/null 2>&1 || die "git is not installed — run 00-install-prereqs.sh first."

hd "Fetching Chipotle"
info "repo:   $REPO"
info "ref:    $REF"
info "target: $TARGET"

if [ -d "$TARGET/.git" ]; then
  info "Existing checkout found — updating..."
  git -C "$TARGET" fetch --all --prune
  git -C "$TARGET" checkout "$REF"
  git -C "$TARGET" pull --ff-only origin "$REF" || warn "fast-forward pull skipped (detached ref or local changes)"
else
  info "Cloning..."
  mkdir -p "$(dirname "$TARGET")"
  git clone "$REPO" "$TARGET"
  git -C "$TARGET" checkout "$REF"
fi

HEAD_SHA="$(git -C "$TARGET" rev-parse --short HEAD)"
ok "Chipotle at $TARGET (HEAD $HEAD_SHA on $REF)"
[ -f "$TARGET/local_test.sh" ] && ok "local_test.sh present" || die "local_test.sh missing — is this the right repo?"

hd "Next"
info "Warm the build so the runtime needs no network:"
info "   CHIPOTLE_DIR=$TARGET bash local-runbook/scripts/03-warm-build.sh"
