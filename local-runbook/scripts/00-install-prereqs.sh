#!/usr/bin/env bash
# 00-install-prereqs.sh — install every base requirement for running Chipotle
# locally, on macOS or Debian/Ubuntu Linux. Idempotent: re-running skips tools
# that are already present.
#
# INSTALL PHASE NEEDS THE INTERNET. The *runtime* does not (that is the whole
# point) — but downloading toolchains, crates, and the dstack simulator does.
#
# Installs: git, jq, Node.js + npm, static-web-server, Rust (rustup) + the 1.91
# toolchain, Foundry (anvil/forge/cast), C build tools, and the dstack simulator
# (cloned + built into $SIMULATOR_DIR). Docker is optional (Jaeger tracing only)
# and is NOT auto-installed.
#
# Usage:  bash local-runbook/scripts/00-install-prereqs.sh
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; source "$DIR/_common.sh"

OS="$(detect_os)"
[ "$OS" = "unsupported" ] && die "Unsupported OS: $(uname -s). This runbook supports macOS and Linux."
hd "Installing Chipotle prerequisites for: $OS"

have() { command -v "$1" >/dev/null 2>&1; }

# ── macOS ─────────────────────────────────────────────────────────────────────
install_macos() {
  if ! have brew; then
    info "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    # Make brew available on Apple Silicon for the rest of this script.
    [ -x /opt/homebrew/bin/brew ] && eval "$(/opt/homebrew/bin/brew shellenv)"
  fi
  ok "Homebrew present"

  if ! xcode-select -p >/dev/null 2>&1; then
    warn "Installing Xcode Command Line Tools (a GUI dialog may appear — accept it, then re-run this script)."
    xcode-select --install || true
  else
    ok "Xcode Command Line Tools present"
  fi

  for pkg in git jq node static-web-server protobuf; do
    if brew list --versions "$pkg" >/dev/null 2>&1; then ok "$pkg present";
    else info "brew install $pkg"; brew install "$pkg"; fi
  done
}

# ── Debian / Ubuntu ───────────────────────────────────────────────────────────
install_linux() {
  if ! have apt-get; then
    die "This installer targets Debian/Ubuntu (apt). For another distro, install the equivalents of: build-essential pkg-config libssl-dev clang git curl jq perl make nodejs npm, then run 01-check-prereqs.sh."
  fi
  info "Updating apt and installing system packages${SUDO:+ (sudo required)}..."
  asroot apt-get update -y
  # procps → pgrep (local_test.sh needs it); gnupg/ca-certificates → NodeSource apt repo
  # protobuf-compiler → protoc (lit-actions-grpc build.rs needs it)
  asroot apt-get install -y git curl jq perl make build-essential pkg-config libssl-dev clang \
    ca-certificates gnupg procps protobuf-compiler
  ok "system packages installed"

  if ! have node; then
    info "Installing Node.js 20 LTS via NodeSource..."
    curl -fsSL https://deb.nodesource.com/setup_20.x -o /tmp/nodesource_setup.sh
    asroot bash /tmp/nodesource_setup.sh
    asroot apt-get install -y nodejs
  fi
  ok "node present ($(node --version 2>/dev/null || echo '?'))"

  if ! have static-web-server; then
    info "Installing static-web-server (prebuilt release binary to /usr/local/bin)..."
    local arch tag url tmp
    case "$(uname -m)" in
      aarch64|arm64) arch="aarch64-unknown-linux-gnu" ;;
      x86_64|amd64)  arch="x86_64-unknown-linux-gnu" ;;
      *) die "Unsupported arch for static-web-server: $(uname -m)" ;;
    esac
    tag="$(curl -sSL https://api.github.com/repos/static-web-server/static-web-server/releases/latest | jq -r .tag_name)"
    [ -n "$tag" ] && [ "$tag" != null ] || die "Could not resolve latest static-web-server release tag."
    url="https://github.com/static-web-server/static-web-server/releases/download/${tag}/static-web-server-${tag}-${arch}.tar.gz"
    tmp="$(mktemp -d)"
    curl -sSL "$url" -o "$tmp/sws.tar.gz" || die "Download failed: $url"
    tar -xzf "$tmp/sws.tar.gz" -C "$tmp"
    asroot install -m 0755 "$(find "$tmp" -type f -name static-web-server | head -1)" /usr/local/bin/static-web-server
    rm -rf "$tmp"
  fi
  ok "static-web-server present ($(static-web-server --version 2>/dev/null | head -1))"
}

# ── Cross-platform: Rust, Foundry, dstack simulator ──────────────────────────
install_rust() {
  if ! have rustup; then
    info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  fi
  # shellcheck disable=SC1091
  [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
  have rustc || die "rustc not on PATH after rustup install — open a new shell and re-run."
  info "Installing pinned toolchain 1.91 (matches rust-toolchain.toml)..."
  rustup toolchain install 1.91 --component rustfmt clippy rust-src >/dev/null 2>&1 || rustup toolchain install 1.91
  ok "Rust present ($(rustc --version)); 1.91 toolchain installed"
}

install_foundry() {
  if ! have foundryup; then
    info "Installing Foundry (foundryup)..."
    curl -L https://foundry.paradigm.xyz | bash
  fi
  export PATH="$HOME/.foundry/bin:$PATH"
  have foundryup || die "foundryup not on PATH — open a new shell and re-run, or add ~/.foundry/bin to PATH."
  info "Running foundryup to install/update anvil, forge, cast..."
  foundryup
  ok "Foundry present ($(anvil --version 2>/dev/null | head -1))"
}

install_dstack_sim() {
  local repo_root="$HOME/GitHub/dstack"
  if [ ! -f "$SIMULATOR_DIR/dstack-simulator" ]; then
    if [ ! -d "$repo_root" ]; then
      info "Cloning dstack (for the simulator) into $repo_root..."
      mkdir -p "$(dirname "$repo_root")"
      git clone https://github.com/Dstack-TEE/dstack.git "$repo_root"
    fi
    info "Building the dstack simulator (needs Rust)..."
    ( cd "$SIMULATOR_DIR" && bash build.sh )
  fi
  [ -f "$SIMULATOR_DIR/dstack-simulator" ] && ok "dstack-simulator built at $SIMULATOR_DIR" \
    || die "dstack-simulator not found at $SIMULATOR_DIR after build."
}

case "$OS" in
  macos) install_macos ;;
  linux) install_linux ;;
esac
install_rust
install_foundry
install_dstack_sim

hd "Optional"
if have docker; then ok "docker present (Jaeger tracing available)";
else warn "docker not installed — Jaeger tracing will be auto-skipped. Everything else works. (macOS: 'brew install --cask docker'; Linux: 'sudo apt-get install docker.io')"; fi

hd "Install complete"
info "Open a NEW shell (to pick up PATH changes), then verify with:"
info "   bash local-runbook/scripts/01-check-prereqs.sh"
