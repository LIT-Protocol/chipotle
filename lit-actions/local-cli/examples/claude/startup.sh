# Install and run Claude Code inside a Lit binary action.
#
# `lit run` executes exactly what the sandbox executes — `bash startup.sh` —
# so this is the same bundle you'd ship to the TEE. In the sandbox the base
# rootfs is "Slimworm" (debian:bookworm-slim) with curl but no Node; the
# native installer drops a standalone binary under ~/.local/bin either way.
#
# NOTE: run locally, this really installs Claude Code onto *your* machine
# (~/.local/bin) and, with a key set, calls api.anthropic.com.
#
# Top-level js-params arrive as environment variables (CPL-355):
#   ANTHROPIC_API_KEY  — Claude API key (optional; install-only run without it)
#   PROMPT             — the prompt to run headlessly (optional)
set -euo pipefail

lit print "installing Claude Code on $(uname -sm)"
curl -fsSL https://claude.ai/install.sh | bash
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
