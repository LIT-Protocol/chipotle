#!/usr/bin/env bash
# Build the FROST wasm-bindgen wrapper for the two targets the example needs:
#   * web    -> imported by the Lit Action (Deno) from a pinned CDN/inlined blob
#   * nodejs -> required by the user-side client (client/mpcClient.js)
#
# Mirrors the two-target build pattern of LIT-Protocol/lit-ecdsa-wasm-combine.
#
# Prereqs: rustup, the wasm32 target, and wasm-pack:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-pack
set -euo pipefail
cd "$(dirname "$0")"

OUT_WEB="pkg-web"
OUT_NODE="pkg-node"

echo "==> web build  -> $OUT_WEB"
wasm-pack build --release --target web    --out-dir "$OUT_WEB"  --out-name lit_frost_wasm

echo "==> node build -> $OUT_NODE"
wasm-pack build --release --target nodejs --out-dir "$OUT_NODE" --out-name lit_frost_wasm

echo
echo "Done."
echo "  Action (web):  $OUT_WEB/lit_frost_wasm.js + .wasm  — publish to npm/jsDelivr or inline the .wasm as base64 in action/mpcSigner.js"
echo "  Client (node): $OUT_NODE/                            — referenced by client/mpcClient.js"
