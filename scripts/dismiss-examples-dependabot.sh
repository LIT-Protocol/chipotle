#!/usr/bin/env bash
set -euo pipefail

# Dismiss every OPEN Dependabot alert whose manifest lives under examples/.
#
# Why a script instead of config: Dependabot's `exclude-paths` (dependabot.yml)
# only suppresses *version-update* PRs — it does NOT keep the path out of the
# Security-tab *alerts* (see dependabot/dependabot-core#14408 and #7522). There
# is no native per-path exclusion for alerts, so the maintainable equivalent is
# to auto-dismiss alerts under examples/. Everything in examples/ is standalone
# demo/sample code that ships in no production artifact, so "not_used" is the
# honest dismissal reason. Dismissed alerts can be reopened from the Security
# tab if that ever changes.
#
# Requires a token with the Dependabot-alerts (security_events) write scope.
# The default Actions GITHUB_TOKEN canNOT dismiss Dependabot alerts, which is
# why this is a local script run with the maintainer's `gh` auth rather than a
# scheduled workflow. Run it periodically (or after adding a new example):
#
#   scripts/dismiss-examples-dependabot.sh            # dismiss
#   DRY_RUN=1 scripts/dismiss-examples-dependabot.sh  # list only, no changes
#
# Optional env:
#   REPO=LIT-Protocol/chipotle   # override target repo
#   PREFIX=examples/             # override the path prefix to dismiss

REPO="${REPO:-LIT-Protocol/chipotle}"
PREFIX="${PREFIX:-examples/}"
DRY_RUN="${DRY_RUN:-0}"
REASON="not_used"
COMMENT="Auto-dismissed: dependency lives under ${PREFIX} (standalone demo/sample code, not shipped). See scripts/dismiss-examples-dependabot.sh."

echo "Fetching open Dependabot alerts for ${REPO} under ${PREFIX} ..."
# Portable (bash 3.2 / macOS): collect alert numbers into a space-separated list.
NUMBERS="$(
  gh api --paginate \
    -H "Accept: application/vnd.github+json" \
    "/repos/${REPO}/dependabot/alerts?state=open&per_page=100" \
    --jq ".[] | select(.dependency.manifest_path | startswith(\"${PREFIX}\")) | .number"
)"

COUNT="$(printf '%s\n' "${NUMBERS}" | grep -c . || true)"
echo "Found ${COUNT} open alert(s) under ${PREFIX}."
if [[ -z "${NUMBERS}" ]]; then
  exit 0
fi

for n in ${NUMBERS}; do
  if [[ "${DRY_RUN}" == "1" ]]; then
    echo "  [dry-run] would dismiss alert #${n}"
    continue
  fi
  echo "  dismissing alert #${n}"
  gh api \
    --method PATCH \
    -H "Accept: application/vnd.github+json" \
    "/repos/${REPO}/dependabot/alerts/${n}" \
    -f "state=dismissed" \
    -f "dismissed_reason=${REASON}" \
    -f "dismissed_comment=${COMMENT}" \
    --jq '.number as $n | "    dismissed #\($n)"'
done

echo "Done."
