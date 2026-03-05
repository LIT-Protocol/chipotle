#!/usr/bin/env bash
set -euo pipefail

# Optional: PR URL (if omitted, find PR for current branch)
# Optional: base branch for diff (default: PR's target branch)
BASE_BRANCH=""

if [[ $# -ge 1 && "$1" == *github.com* ]]; then
  PR_URL="$1"
  # Parse owner/repo and PR number from the URL
  if [[ "$PR_URL" =~ github\.com/([^/]+/[^/]+)/pull/([0-9]+) ]]; then
    REPO="${BASH_REMATCH[1]}"
    PR_NUMBER="${BASH_REMATCH[2]}"
  else
    echo "Error: Could not parse PR URL. Expected format: https://github.com/owner/repo/pull/123"
    exit 1
  fi
  [[ $# -ge 2 ]] && BASE_BRANCH="$2"
else
  # No PR URL or first arg is base branch: find PR for current branch
  if [[ $# -ge 2 ]]; then
    BASE_BRANCH="$2"
  elif [[ $# -ge 1 && -n "$1" && "$1" != *github.com* ]]; then
    BASE_BRANCH="$1"
  fi
  if ! REPO=$(gh pr view --json baseRepository -q '.baseRepository.nameWithOwner' 2>/dev/null); then
    echo "Error: No PR URL given and no PR found for the current branch."
    echo "Usage: $0 [github-pr-url] [base-branch]"
    echo "  PR URL optional: omit to use the PR for the current branch."
    echo "  Base branch optional: default is the PR's target branch. Example: $0 '' develop"
    echo "Example: $0 https://github.com/owner/repo/pull/123"
    echo "Example: $0 '' develop   # current branch's PR, diff against develop"
    exit 1
  fi
  PR_NUMBER=$(gh pr view --json number -q '.number')
fi

# Default base branch to the PR's target (merge target) when not specified
if [[ -z "${BASE_BRANCH:-}" ]]; then
  BASE_BRANCH=$(gh pr view "$PR_NUMBER" --repo "$REPO" --json baseRefName -q '.baseRefName')
fi

echo "Reviewing PR #${PR_NUMBER} on ${REPO} (diff against ${BASE_BRANCH})..."

# Get the PR title for context
PR_TITLE=$(gh pr view "$PR_NUMBER" --repo "$REPO" --json title --jq '.title')

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

# Try local git diff first (faster, supports custom base branch), fall back to
# gh pr diff for fork PRs where the head branch doesn't exist on origin.
HEAD_REF=$(gh pr view "$PR_NUMBER" --repo "$REPO" --json headRefName -q '.headRefName')
DIFF_FROM_GIT=0

if git fetch origin "$BASE_BRANCH" "$HEAD_REF" --quiet 2>/dev/null; then
  git diff "origin/${BASE_BRANCH}...origin/${HEAD_REF}" > "$TMPDIR/pr.diff"
  [[ -s "$TMPDIR/pr.diff" ]] && DIFF_FROM_GIT=1
fi

if [[ ! -s "$TMPDIR/pr.diff" ]]; then
  echo "Local fetch failed (fork PR?), falling back to gh pr diff..."
  gh pr diff "$PR_NUMBER" --repo "$REPO" > "$TMPDIR/pr.diff"
fi

if [[ ! -s "$TMPDIR/pr.diff" ]]; then
  echo "Error: No diff found for PR #${PR_NUMBER}"
  exit 1
fi

OUTFILE="$TMPDIR/review.md"

echo "Running codex review..."

if [[ "$DIFF_FROM_GIT" -eq 1 ]]; then
  # We have the PR branch locally: use codex review with --base (no custom prompt)
  PREV=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || git rev-parse HEAD)
  trap 'git checkout "$PREV" 2>/dev/null; rm -rf "$TMPDIR"' EXIT
  git checkout "origin/${HEAD_REF}" --quiet
  codex review --base "origin/${BASE_BRANCH}" --title "$PR_TITLE" 2>&1 | tee "$TMPDIR/raw.txt" | awk '/^codex$/ { buf=""; next } { buf=buf $0 "\n" } END { printf "%s", buf }' > "$OUTFILE" || true
  git checkout "$PREV" --quiet
  trap 'rm -rf "$TMPDIR"' EXIT
else
  # Fork PR or no local branch: pass diff via stdin as custom instructions
  {
    echo "Review the following PR diff."
    echo ""
    cat "$TMPDIR/pr.diff"
  } | codex review - 2>&1 | tee "$TMPDIR/raw.txt" | awk '/^codex$/ { buf=""; next } { buf=buf $0 "\n" } END { printf "%s", buf }' > "$OUTFILE" || true
fi

# If extraction left empty, use raw output (e.g. different codex output format)
if [[ ! -s "$OUTFILE" && -s "$TMPDIR/raw.txt" ]]; then
  cp "$TMPDIR/raw.txt" "$OUTFILE"
fi

if [[ ! -s "$OUTFILE" ]]; then
  echo "Error: Codex returned an empty review"
  exit 1
fi

REVIEW=$(cat "$OUTFILE")

# Post the review as a PR comment
gh pr comment "$PR_NUMBER" --repo "$REPO" --body "$(cat <<EOF
## 🤖 Codex PR Review

${REVIEW}

---
*Automated review by [Codex](https://github.com/openai/codex)*
EOF
)"

echo "Review posted to PR #${PR_NUMBER} on ${REPO}"
