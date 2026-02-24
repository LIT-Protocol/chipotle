#!/usr/bin/env sh
# Integration tests against a deployed lit-api-server instance.
# Usage:
#   API_BASE_URL=https://your-deployed-instance.network just test-deployed-api
#   just test-deployed-api  # uses default Phala deploy URL
#
# Requires: curl, jq

set -e

API_BASE_URL="${API_BASE_URL:-https://36da669c852c9bd4fdea27dd331c07ff776bd125-8000.dstack-pha-prod5.phala.network}"
CORE_V1="${API_BASE_URL}/core/v1"
FAILED=0

echo "Testing deployed API at: ${API_BASE_URL}"
echo ""

# Helper: GET request, expect 200 and valid JSON. Prints PASS/FAIL to stderr, body to stdout on success.
get_ok() {
  local name="$1"
  local url="$2"
  local res
  res=$(curl -sS -w "\n%{http_code}" "$url")
  local body
  body=$(echo "$res" | head -n -1)
  local code
  code=$(echo "$res" | tail -n 1)
  if [ "$code" != "200" ]; then
    echo "FAIL: $name - HTTP $code" >&2
    echo "  Response: $body" >&2
    FAILED=1
    return 1
  fi
  if ! echo "$body" | jq -e . >/dev/null 2>&1; then
    echo "FAIL: $name - invalid JSON" >&2
    echo "  Response: $body" >&2
    FAILED=1
    return 1
  fi
  echo "PASS: $name" >&2
  echo "$body"
}

# Helper: POST request, expect 200 and valid JSON. Prints PASS/FAIL to stderr, body to stdout on success.
post_ok() {
  local name="$1"
  local url="$2"
  local data="$3"
  local res
  res=$(curl -sS -w "\n%{http_code}" -X POST -H "Content-Type: application/json" -d "$data" "$url")
  local body
  body=$(echo "$res" | head -n -1)
  local code
  code=$(echo "$res" | tail -n 1)
  if [ "$code" != "200" ]; then
    echo "FAIL: $name - HTTP $code" >&2
    echo "  Response: $body" >&2
    FAILED=1
    return 1
  fi
  if ! echo "$body" | jq -e . >/dev/null 2>&1; then
    echo "FAIL: $name - invalid JSON" >&2
    echo "  Response: $body" >&2
    FAILED=1
    return 1
  fi
  echo "PASS: $name" >&2
  echo "$body"
}

# 1. OpenAPI spec is served
echo "--- OpenAPI spec ---"
get_ok "GET /openapi.json" "${API_BASE_URL}/openapi.json" >/dev/null
echo ""

# 2. get_node_chain_config (no auth, config-only)
echo "--- Core v1 endpoints ---"
CHAIN_CFG=$(get_ok "GET /core/v1/get_node_chain_config" "${CORE_V1}/get_node_chain_config")
if [ -n "$CHAIN_CFG" ]; then
  CHAIN_NAME=$(echo "$CHAIN_CFG" | jq -r '.chain_name // empty')
  CHAIN_ID=$(echo "$CHAIN_CFG" | jq -r '.chain_id // empty')
  echo "  chain_name=$CHAIN_NAME chain_id=$CHAIN_ID"
fi
echo ""

# 3. get_lit_action_ipfs_id (pure computation, no blockchain)
CODE="console.log(1)"
# URL-encode the code for path (replace / with %2F, etc.)
ENCODED_CODE=$(printf '%s' "$CODE" | jq -sRr @uri)
get_ok "GET /core/v1/get_lit_action_ipfs_id/<code>" "${CORE_V1}/get_lit_action_ipfs_id/${ENCODED_CODE}" >/dev/null
echo ""

# 4. new_account (creates account; may fail if blockchain/RPC not configured)
echo "--- Account flow (may fail if blockchain not configured) ---"
NEW_ACCOUNT=$(post_ok "POST /core/v1/new_account" "${CORE_V1}/new_account" '{"account_name":"test-deploy","account_description":"integration test"}')
if [ -n "$NEW_ACCOUNT" ]; then
  API_KEY=$(echo "$NEW_ACCOUNT" | jq -r '.api_key // empty')
  if [ -n "$API_KEY" ]; then
    # 5. account_exists (URL-encode api_key for path)
    ENCODED_KEY=$(printf '%s' "$API_KEY" | jq -sRr @uri)
    get_ok "GET /core/v1/account_exists/<api_key>" "${CORE_V1}/account_exists/${ENCODED_KEY}" >/dev/null
    # 6. create_wallet (GET /create_wallet/<api_key>)
    get_ok "GET /core/v1/create_wallet/<api_key>" "${CORE_V1}/create_wallet/${ENCODED_KEY}" >/dev/null
  fi
else
  echo "  (skipping account_exists, create_wallet - new_account failed)"
fi
echo ""

if [ $FAILED -eq 0 ]; then
  echo "All tests passed."
  exit 0
else
  echo "Some tests failed."
  exit 1
fi
