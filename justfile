# Conventions: https://github.com/casey/just

image_base := env('DOCKER_IMAGE', 'litptcl/lit-node-express')
# Tag used only to satisfy the registry push requirement; the deploy step uses
# the @sha256: digest captured after push, never this tag. Override with
# DOCKER_TAG to reuse a previously-pushed image (digest files must then exist).
image_tag := env('DOCKER_TAG', `uuidgen | tr '[:upper:]' '[:lower:]' | tr -d '\n'`)
image_lit_actions     := image_base + '-lit-actions:'     + image_tag
image_lit_api_server  := image_base + '-lit-api-server:'  + image_tag
image_lit_static      := image_base + '-lit-static:'      + image_tag
image_otel_collector  := image_base + '-otel-collector:'  + image_tag
# main → chipotle-dev; any other branch → chipotle-next (override with PHALA_APP_NAME)
app_name       := `git branch --show-current | xargs -I {} sh -c '[ "{}" = "main" ] && echo chipotle-dev || echo chipotle-next'`
instance_type  := `git branch --show-current | xargs -I {} sh -c '[ "{}" = "main" ] && echo tdx.small || echo tdx.small'`
gcp_project_id := `git branch --show-current | xargs -I {} sh -c '[ "{}" = "main" ] && echo chipotle-dev || echo chipotle-next'`
base_url       := `git branch --show-current | xargs -I {} sh -c '[ "{}" = "main" ] && echo https://f8fce543471dc9f5f5643aa217422398c36e5edc-8000.dstack-base-prod5.phala.network || echo https://969a8c14c9e13420705b19c7246aeed27897e7ea-8000.dstack-base-prod5.phala.network'`

import "justfile.deploy"
import "justfile.sim"

# List available recipes (default when invoked with no args)
default:
    @just --list

[group: 'build']
build: actions api-server

[group: 'build']
actions:
    cargo build --manifest-path=lit-actions/Cargo.toml --bin lit_actions --all-features

[group: 'build']
api-server:
    cargo build --manifest-path=lit-api-server/Cargo.toml --bin lit-api-server --all-features

clean:
    cargo clean --manifest-path=lit-actions/Cargo.toml
    cargo clean --manifest-path=lit-api-server/Cargo.toml

[group: 'build']
fmt:
    #!/usr/bin/env sh
    set -eu
    find . -name Cargo.toml -not -path './.claude/*' -not -path '*node_modules*' | while read f; do
        cargo fmt --manifest-path="$f" --all
    done

[group: 'build']
clippy:
    #!/usr/bin/env sh
    set -eu
    find . -name Cargo.toml -not -path './.claude/*' -not -path '*node_modules*' | while read f; do
        cargo clippy --manifest-path="$f" --all --all-targets --fix --allow-dirty -- -D warnings
    done

[group: 'debug']
ssh:
    phala ssh
# Run k6 load tests. Default: smoke. Pass test names to run others.
# Usage:
#   just test                    # runs smoke
#   just test integration        # runs integration suite
#   just test sample             # runs k6-script.sample.ts
#   just test ecdsa-sign         # runs lit-action-ecdsa-sign.spec.ts
#   just test smoke integration  # runs both
#   BASE_URL=.../core/v1 just test
# Generate Rust bindings from Solidity artifacts (compile + generate).
[group: 'contracts']
contracts-generate:
    make -C lit-api-server/blockchain/lit_node_express generate

# Deploy contracts to Base Sepolia (generate + deploy).
[group: 'contracts']
contracts-deploy:
    make -C lit-api-server/blockchain/lit_node_express deploy_base

[group: 'test']
test *names='smoke':
    #!/usr/bin/env sh
    set -eu
    command -v k6 >/dev/null 2>&1 || { echo "error: k6 not found. Install from https://grafana.com/docs/k6/latest/set-up/install-k6/"; exit 1; }
    for t in {{names}}; do
        case "$t" in
            smoke)       k6 run k6/smoke.spec.ts ;;
            integration) k6 run k6/integration.spec.ts ;;
            ecdsa-sign)  k6 run k6/lit-action-ecdsa-sign.spec.ts ;;
            sample)      k6 run k6/k6-script.sample.ts ;;
            *) echo "error: unknown test '$t'. Available: smoke, integration, ecdsa-sign, sample"; exit 1 ;;
        esac
    done
