#!/usr/bin/env bash
# 05-verify-pkp-sign.sh — end-to-end local verification against a running stack.
# Exercises the full account lifecycle and proves a Lit Action can sign, twice:
#
#   PART A (action-key sign):
#     1. create account            POST /new_account
#     2. create usage key          POST /add_usage_api_key   (execute_in_groups)
#     3. run signing action        POST /lit_action          (signs with the
#                                                             action's CID-derived key)
#
#   PART B (PKP sign — the important one):
#     1. create account            POST /new_account
#     2. mint PKP (the signer)     POST /create_wallet
#     3. action CID                POST /get_lit_action_ipfs_id
#     4. create group              POST /add_group
#     5. register action           POST /add_action
#     6. add action -> group       POST /add_action_to_group
#     7. add PKP -> group          POST /add_pkp_to_group      (authorizes CID->PKP)
#     8. usage key (exec in group) POST /add_usage_api_key
#     9. run action w/ js_params.pkpId  POST /lit_action
#    -> VERIFIES the returned signer address == the minted PKP address.
#
# Requires the stack to be up (04-run-local.sh). No internet needed.
# NOTE: runs under bash on purpose — do not `zsh` this file (GID is reserved in zsh).
#
# Usage:  bash local-runbook/scripts/05-verify-pkp-sign.sh
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; source "$DIR/_common.sh"
set +e  # this is a test: keep going and tally results, don't abort on first failure
LIB="$(cd "$DIR/../lib" && pwd)"
BASE="$BASE_URL_LOCAL"

command -v jq >/dev/null 2>&1 || die "jq is required."
curl -sf "http://localhost:${API_PORT}/core/v1/health" >/dev/null 2>&1 \
  || die "lit-api-server is not healthy on :$API_PORT. Start it: bash local-runbook/scripts/04-run-local.sh"

# lower-case helper (portable; avoids bash 4 ${var,,})
lc() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

api() { # METHOD PATH [DATA] [EXTRA_HEADER...]
  local method="$1" path="$2" data="${3:-}"; shift || true; shift || true; shift || true
  if [ -n "$data" ]; then
    curl -sS -X "$method" "$BASE$path" -H 'Content-Type: application/json' "$@" --data-binary "$data"
  else
    curl -sS -X "$method" "$BASE$path" -H 'Content-Type: application/json' "$@"
  fi
}

########################################################################
hd "PART A — sign with the ACTION's own (CID-derived) key"
########################################################################
ACCT=$(api POST /new_account '{"account_name":"runbook-a","account_description":"action-key sign"}')
A_KEY=$(echo "$ACCT" | jq -r .api_key)
[ "$A_KEY" != null ] && ok "account created" || { fail "new_account failed: $ACCT"; }

A_USAGE=$(api POST /add_usage_api_key \
  '{"name":"a-usage","description":"exec","can_create_groups":false,"can_delete_groups":false,"can_create_pkps":false,"manage_ipfs_ids_in_groups":[],"add_pkp_to_groups":[],"remove_pkp_from_groups":[],"execute_in_groups":[0]}' \
  -H "X-Api-Key: $A_KEY" | jq -r .usage_api_key)
[ -n "$A_USAGE" ] && [ "$A_USAGE" != null ] && ok "usage key created (execute_in_groups=[0])" || fail "add_usage_api_key failed"

A_RUN=$(jq -Rs --argjson n null '{code:.,js_params:$n}' < "$LIB/action-key-sign.js" \
  | api POST /lit_action "$(cat)" -H "X-Api-Key: $A_USAGE")
A_ERR=$(echo "$A_RUN" | jq -r .has_error)
A_SIGNER=$(echo "$A_RUN" | jq -r '.response.signer_wallet_address // empty')
if [ "$A_ERR" = "false" ] && [ -n "$A_SIGNER" ]; then
  ok "action ran: has_error=false, signer=$A_SIGNER"
else
  fail "action-key sign failed: $A_RUN"
fi

########################################################################
hd "PART B — sign with the MINTED PKP"
########################################################################
hd "B1. create account"
ACCT=$(api POST /new_account '{"account_name":"runbook-b","account_description":"pkp sign"}')
API_KEY=$(echo "$ACCT" | jq -r .api_key)
ACCT_WALLET=$(echo "$ACCT" | jq -r .wallet_address)
[ "$API_KEY" != null ] && ok "account wallet=$ACCT_WALLET" || { fail "new_account failed: $ACCT"; summary; exit 1; }
AUTH=(-H "X-Api-Key: $API_KEY")

hd "B2. mint PKP (the signer)"
PKP_ADDR=$(api POST /create_wallet '' "${AUTH[@]}" | jq -r .wallet_address)
[ -n "$PKP_ADDR" ] && [ "$PKP_ADDR" != null ] && ok "PKP=$PKP_ADDR" || fail "create_wallet failed"

hd "B3. compute action CID"
CID=$(jq -Rs . < "$LIB/pkp-sign.js" | api POST /get_lit_action_ipfs_id "$(cat)" | tr -d '"')
[ -n "$CID" ] && ok "CID=$CID" || fail "get_lit_action_ipfs_id failed"

hd "B4. create group"
GROUP_ID=$(api POST /add_group \
  '{"group_name":"runbook-group","group_description":"grp","pkp_ids_permitted":[],"cid_hashes_permitted":[]}' \
  "${AUTH[@]}" | jq -r .group_id)
[ -n "$GROUP_ID" ] && [ "$GROUP_ID" != null ] && ok "group_id=$GROUP_ID" || fail "add_group failed"

hd "B5. register action"
R=$(api POST /add_action "$(jq -nc --arg c "$CID" '{action_ipfs_cid:$c,name:"pkp-sign",description:"pkp sign"}')" "${AUTH[@]}")
[ "$(echo "$R" | jq -r .success)" = true ] && ok "add_action success" || fail "add_action: $R"

hd "B6. add action -> group"
R=$(api POST /add_action_to_group "$(jq -nc --arg c "$CID" --argjson g "$GROUP_ID" '{group_id:$g,action_ipfs_cid:$c}')" "${AUTH[@]}")
[ "$(echo "$R" | jq -r .success)" = true ] && ok "add_action_to_group success" || fail "add_action_to_group: $R"

hd "B7. add PKP -> group (authorizes CID->PKP via canUseWalletInAction)"
R=$(api POST /add_pkp_to_group "$(jq -nc --arg p "$PKP_ADDR" --argjson g "$GROUP_ID" '{group_id:$g,pkp_id:$p}')" "${AUTH[@]}")
[ "$(echo "$R" | jq -r .success)" = true ] && ok "add_pkp_to_group success" || fail "add_pkp_to_group: $R"

hd "B8. create usage key authorized to execute in group $GROUP_ID"
USAGE_KEY=$(api POST /add_usage_api_key \
  "$(jq -nc --argjson g "$GROUP_ID" '{name:"b-usage",description:"exec",can_create_groups:false,can_delete_groups:false,can_create_pkps:false,manage_ipfs_ids_in_groups:[],add_pkp_to_groups:[],remove_pkp_from_groups:[],execute_in_groups:[$g]}')" \
  "${AUTH[@]}" | jq -r .usage_api_key)
[ -n "$USAGE_KEY" ] && [ "$USAGE_KEY" != null ] && ok "usage key created" || fail "add_usage_api_key failed"

hd "B9. RUN the action, signing with the PKP (js_params.pkpId)"
RUN=$(jq -Rs --arg pkp "$PKP_ADDR" '{code:.,js_params:{pkpId:$pkp}}' < "$LIB/pkp-sign.js" \
  | api POST /lit_action "$(cat)" -H "X-Api-Key: $USAGE_KEY")
echo "$RUN" | jq . 2>/dev/null || echo "$RUN"
ERR=$(echo "$RUN" | jq -r .has_error 2>/dev/null || echo true)
SIGNER=$(echo "$RUN" | jq -r '.response.signer_wallet_address // empty' 2>/dev/null || echo "")

hd "Verification"
printf "   %-24s %s\n" "minted PKP address:"    "$PKP_ADDR"
printf "   %-24s %s\n" "action signer address:" "${SIGNER:-<none>}"
printf "   %-24s %s\n" "has_error:"             "$ERR"
if [ "$ERR" = "false" ] && [ -n "$SIGNER" ] && [ "$(lc "$SIGNER")" = "$(lc "$PKP_ADDR")" ]; then
  ok "MATCH — the signature was produced by the minted PKP"
else
  fail "MISMATCH or error — signer != minted PKP (or action errored)"
fi

summary && { hd "LOCAL VERIFICATION PASSED"; exit 0; } || { hd "LOCAL VERIFICATION FAILED"; exit 1; }
