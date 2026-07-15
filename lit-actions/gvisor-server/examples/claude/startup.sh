# Install and run Claude Code inside a Lit binary action.
#
# The sandbox base rootfs ("Slimworm" = debian:bookworm-slim, see
# ../../image/Dockerfile.rootfs) ships curl + ca-certificates but no Node, so
# we use Claude Code's native installer — it drops a standalone binary under
# ~/.local/bin. The sandbox only ever runs `bash startup.sh`; installing and
# launching Claude is this script's job.
#
# Top-level js-params arrive as environment variables (CPL-355):
#   ANTHROPIC_API_KEY  — Claude API key (optional; install-only run without it)
#   PROMPT             — the prompt to run headlessly (optional)
#
# Requires sandbox network egress to claude.ai (install) and, when a key is
# supplied, api.anthropic.com (the prompt).
set -euo pipefail

lit print "installing Claude Code on $(. /etc/os-release && echo "$PRETTY_NAME")"
INSTALLER="$(mktemp)"
curl -fsSL https://claude.ai/install.sh -o "$INSTALLER"
bash "$INSTALLER"
rm -f "$INSTALLER"
export PATH="$HOME/.local/bin:$PATH"

VERSION="$(claude --version)"
lit print "installed Claude Code: $VERSION"

# No key ⇒ prove the install worked and stop (exit 0 returns success).
if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
  lit print "no ANTHROPIC_API_KEY param — install-only run"
  lit set-response "$(jq -n --arg v "$VERSION" '{installed: true, version: $v}')"
  exit 0
fi

# Headless run: `-p` prints the reply and exits. Meter the outbound call the
# same way any action fetch is metered.
lit increment-fetch-count >/dev/null
REPLY="$(claude -p "${PROMPT:-Say hello from a Lit binary action in one sentence.}")"

lit print "claude replied: $REPLY"
lit set-response "$(jq -n --arg v "$VERSION" --arg r "$REPLY" '{ok: true, version: $v, reply: $r}')"
