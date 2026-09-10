#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/sdk"

npm login --scope=@lit-protocol --registry=https://registry.npmjs.org/
npm run build
npm publish --access public --registry=https://registry.npmjs.org/ "$@"
