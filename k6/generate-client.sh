#!/usr/bin/env bash
# Usage: bash k6/generate-client.sh [spec file or URL] [output directory]
# With no spec, build it from this checkout's Rust sources.
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
output_dir=${2:-"$repo_root/k6"}
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT
spec=${1:-}
if [[ -z "$spec" ]]; then
  cargo run --locked --manifest-path "$repo_root/lit-api-server/Cargo.toml" \
    --bin openapi_spec > "$work_dir/openapi.json"
  spec="$work_dir/openapi.json"
fi
# Generate in isolation: the tool also emits support files and must not
# overwrite our handwritten tests or leave stale files on self-hosted runners.
npx --yes @grafana/openapi-to-k6@0.4.1 "$spec" "$work_dir/generated"
npx --yes prettier@3.9.5 --write "$work_dir/generated/litApiServer.ts"
mkdir -p "$output_dir"
cp "$work_dir/generated/litApiServer.ts" "$output_dir/litApiServer.ts"
