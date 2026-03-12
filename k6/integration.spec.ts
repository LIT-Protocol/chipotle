/**
 * Integration tests for lit-api-server.
 * Exercises every endpoint in the OpenAPI spec using a full account lifecycle.
 *
 * Usage:
 *   k6 run k6/integration.spec.ts
 *   BASE_URL=https://your-instance.phala.network/core/v1 k6 run k6/integration.spec.ts
 */
import type { Response } from "k6/http";
import { checkAndLog } from "./check.ts";
import { LitApiServerClient } from "./litApiServer.ts";

const BASE_URL =
  __ENV.BASE_URL ||
  "https://e364da71b0c9af3b9068daa6321edd6ee932aa89-8000.dstack-pha-prod5.phala.network/core/v1";

const HELLO_WORLD_CODE = 'Lit.Actions.setResponse({response: "Hello World!"})';

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_req_failed: ["rate<0.1"],
    http_req_duration: ["p(99)<30000"],
    http_reqs: ["count>=1"],
    checks: ["rate==1"],
  },
};

function assertOk(
  name: string,
  endpoint: string,
  res: { response: Response },
): boolean {
  const { response } = res;
  const status = response?.status ?? 0;
  const ok = status >= 200 && status < 300;
  if (!ok) {
    let msg = "";
    if (status === 0) {
      msg = "(no response / connection failed)";
    } else {
      try {
        const body = JSON.parse(response.body as string);
        msg =
          body.message ??
          body.error ??
          body.detail ??
          (typeof body === "string" ? body : JSON.stringify(body));
      } catch {
        msg = (response.body as string) || "(no body)";
      }
    }
    console.error(`FAIL ${name} | ${endpoint} | ${status} | ${msg}`);
  }
  checkAndLog(response, {
    [`${name} 2xx`]: (r) =>
      (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, name);
  return ok;
}

export default function () {
  const client = new LitApiServerClient({ baseUrl: BASE_URL });

  // ── 1. getNodeChainConfig ─────────────────────────────────────────────────
  const chainConfigRes = client.getNodeChainConfig();
  if (!assertOk("getNodeChainConfig", "GET /get_node_chain_config", chainConfigRes)) return;
  checkAndLog(chainConfigRes.response, {
    "chain config has numeric chain_id": (r) => {
      try {
        return typeof JSON.parse(r.body as string).chain_id === "number";
      } catch {
        return false;
      }
    },
  }, "getNodeChainConfig");

  // ── 2. getLitActionIpfsId ─────────────────────────────────────────────────
  const ipfsRes = client.getLitActionIpfsId(encodeURIComponent(HELLO_WORLD_CODE));
  if (!assertOk("getLitActionIpfsId", "GET /get_lit_action_ipfs_id/{code}", ipfsRes)) return;
  const ipfsId = (ipfsRes.response.body as string).replace(/^"|"$/g, "").trim();
  checkAndLog(ipfsRes.response, {
    "ipfs id is non-empty string": () => ipfsId.length > 0,
  }, "getLitActionIpfsId");

  // ── 3. newAccount ─────────────────────────────────────────────────────────
  const newAccountRes = client.newAccount({
    account_name: "k6-integration-test",
    account_description: "Integration test account",
  });
  if (!assertOk("newAccount", "POST /new_account", newAccountRes)) return;
  const newAccountData = newAccountRes.data as { api_key: string; wallet_address: string };
  checkAndLog(newAccountRes.response, {
    "newAccount returns api_key": () =>
      typeof newAccountData.api_key === "string" && newAccountData.api_key.length > 0,
    "newAccount returns wallet_address": () =>
      typeof newAccountData.wallet_address === "string" && newAccountData.wallet_address.length > 0,
  }, "newAccount");
  const authHeaders = { "X-Api-Key": newAccountData.api_key };

  // ── 4. accountExists ──────────────────────────────────────────────────────
  const existsRes = client.accountExists(authHeaders);
  if (!assertOk("accountExists", "GET /account_exists", existsRes)) return;
  checkAndLog(existsRes.response, {
    "accountExists returns true": (r) => {
      try {
        return JSON.parse(r.body as string) === true;
      } catch {
        return false;
      }
    },
  }, "accountExists");

  // ── 5. createWallet ───────────────────────────────────────────────────────
  const createWalletRes = client.createWallet(authHeaders);
  if (!assertOk("createWallet", "GET /create_wallet", createWalletRes)) return;
  const walletData = createWalletRes.data as { wallet_address: string };
  checkAndLog(createWalletRes.response, {
    "createWallet returns wallet_address": () =>
      typeof walletData.wallet_address === "string" && walletData.wallet_address.length > 0,
  }, "createWallet");
  const walletAddress = walletData.wallet_address;

  // ── 6. listWallets ────────────────────────────────────────────────────────
  const listWalletsRes = client.listWallets(
    { page_number: "0", page_size: "10" },
    authHeaders,
  );
  if (!assertOk("listWallets", "GET /list_wallets", listWalletsRes)) return;
  checkAndLog(listWalletsRes.response, {
    "listWallets returns array": (r) => {
      try {
        return Array.isArray(JSON.parse(r.body as string));
      } catch {
        return false;
      }
    },
  }, "listWallets");

  // ── 7. addGroup ───────────────────────────────────────────────────────────
  const addGroupRes = client.addGroup(
    {
      group_name: "k6-test-group",
      group_description: "Integration test group",
      permitted_actions: [],
      pkps: [],
      all_wallets_permitted: true,
      all_actions_permitted: true,
    },
    authHeaders,
  );
  if (!assertOk("addGroup", "POST /add_group", addGroupRes)) return;
  checkAndLog(addGroupRes.response, {
    "addGroup success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  }, "addGroup");

  // ── 8. listGroups — extract groupId for subsequent tests ──────────────────
  const listGroupsRes = client.listGroups(
    { page_number: "0", page_size: "10" },
    authHeaders,
  );
  if (!assertOk("listGroups", "GET /list_groups", listGroupsRes)) return;
  checkAndLog(listGroupsRes.response, {
    "listGroups returns array": (r) => {
      try {
        return Array.isArray(JSON.parse(r.body as string));
      } catch {
        return false;
      }
    },
  }, "listGroups");
  const groups = listGroupsRes.data as Array<{ id: string; name: string; description: string }>;
  if (!groups || groups.length === 0) {
    console.error("listGroups returned empty array after addGroup");
    return;
  }
  const groupId = groups[groups.length - 1].id; // use the most recently created group

  // ── 9. addActionToGroup ───────────────────────────────────────────────────
  const addActionRes = client.addActionToGroup(
    {
      group_id: groupId,
      action_ipfs_cid: ipfsId,
      name: "hello-world",
      description: "Hello World lit action",
    },
    authHeaders,
  );
  if (!assertOk("addActionToGroup", "POST /add_action_to_group", addActionRes)) return;
  checkAndLog(addActionRes.response, {
    "addActionToGroup success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  }, "addActionToGroup");

  // ── 10. listActions ───────────────────────────────────────────────────────
  const listActionsRes = client.listActions(
    { group_id: groupId, page_number: "0", page_size: "10" },
    authHeaders,
  );
  if (!assertOk("listActions", "GET /list_actions", listActionsRes)) return;
  checkAndLog(listActionsRes.response, {
    "listActions returns array": (r) => {
      try {
        return Array.isArray(JSON.parse(r.body as string));
      } catch {
        return false;
      }
    },
  }, "listActions");

  // ── 11. addPkpToGroup ─────────────────────────────────────────────────────
  const addPkpRes = client.addPkpToGroup(
    { group_id: groupId, pkp_id: walletAddress },
    authHeaders,
  );
  if (!assertOk("addPkpToGroup", "POST /add_pkp_to_group", addPkpRes)) return;
  checkAndLog(addPkpRes.response, {
    "addPkpToGroup success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  }, "addPkpToGroup");

  // ── 12. listWalletsInGroup ────────────────────────────────────────────────
  const listWiGRes = client.listWalletsInGroup(
    { group_id: groupId, page_number: "0", page_size: "10" },
    authHeaders,
  );
  if (!assertOk("listWalletsInGroup", "GET /list_wallets_in_group", listWiGRes)) return;
  checkAndLog(listWiGRes.response, {
    "listWalletsInGroup returns array": (r) => {
      try {
        return Array.isArray(JSON.parse(r.body as string));
      } catch {
        return false;
      }
    },
  }, "listWalletsInGroup");

  // ── 13. litAction ─────────────────────────────────────────────────────────
  const litActionRes = client.litAction(
    { code: HELLO_WORLD_CODE, js_params: null },
    authHeaders,
  );
  if (!assertOk("litAction", "POST /lit_action", litActionRes)) return;
  checkAndLog(litActionRes.response, {
    "litAction has no error": (r) => {
      try {
        return JSON.parse(r.body as string).has_error === false;
      } catch {
        return false;
      }
    },
    "litAction response is Hello World!": (r) => {
      try {
        return JSON.parse(r.body as string).response === "Hello World!";
      } catch {
        return false;
      }
    },
  }, "litAction");

  // ── 14. addUsageApiKey ────────────────────────────────────────────────────
  const expiration = String(Math.floor(Date.now() / 1000) + 86400); // 24 h from now
  const addUsageKeyRes = client.addUsageApiKey(
    { expiration, balance: "1000000000000000000", name: "k6-usage-key", description: "Integration test usage key" }, // 1 token in wei
    authHeaders,
  );
  if (!assertOk("addUsageApiKey", "POST /add_usage_api_key", addUsageKeyRes)) return;
  checkAndLog(addUsageKeyRes.response, {
    "addUsageApiKey success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
    "addUsageApiKey returns usage_api_key": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        return typeof body.usage_api_key === "string" && body.usage_api_key.length > 0;
      } catch {
        return false;
      }
    },
  }, "addUsageApiKey");
  const usageApiKey = (addUsageKeyRes.data as { usage_api_key: string }).usage_api_key;

  // ── 15. listApiKeys ───────────────────────────────────────────────────────
  const listApiKeysRes = client.listApiKeys(
    { page_number: "0", page_size: "10" },
    authHeaders,
  );
  if (!assertOk("listApiKeys", "GET /list_api_keys", listApiKeysRes)) return;
  checkAndLog(listApiKeysRes.response, {
    "listApiKeys returns array": (r) => {
      try {
        return Array.isArray(JSON.parse(r.body as string));
      } catch {
        return false;
      }
    },
  }, "listApiKeys");

  // ── 16. updateGroup ───────────────────────────────────────────────────────
  const updateGroupRes = client.updateGroup(
    {
      group_id: groupId,
      name: "k6-test-group-updated",
      description: "Updated integration test group",
      all_wallets_permitted: true,
      all_actions_permitted: true,
    },
    authHeaders,
  );
  if (!assertOk("updateGroup", "POST /update_group", updateGroupRes)) return;
  checkAndLog(updateGroupRes.response, {
    "updateGroup success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  }, "updateGroup");

  // ── 17. updateActionMetadata ──────────────────────────────────────────────
  const updateActionRes = client.updateActionMetadata(
    {
      group_id: groupId,
      action_ipfs_cid: ipfsId,
      name: "hello-world-updated",
      description: "Updated Hello World lit action",
    },
    authHeaders,
  );
  if (!assertOk("updateActionMetadata", "POST /update_action_metadata", updateActionRes)) return;
  checkAndLog(updateActionRes.response, {
    "updateActionMetadata success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  }, "updateActionMetadata");

  // ── 18. updateUsageApiKeyMetadata ─────────────────────────────────────────
  const updateUsageKeyRes = client.updateUsageApiKeyMetadata(
    { usage_api_key: usageApiKey, name: "k6-usage-key", description: "Integration test usage key" },
    authHeaders,
  );
  if (!assertOk("updateUsageApiKeyMetadata", "POST /update_usage_api_key_metadata", updateUsageKeyRes)) return;
  checkAndLog(updateUsageKeyRes.response, {
    "updateUsageApiKeyMetadata success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  }, "updateUsageApiKeyMetadata");

  // ── 19. removeUsageApiKey ─────────────────────────────────────────────────
  const removeUsageKeyRes = client.removeUsageApiKey(
    { usage_api_key: usageApiKey },
    authHeaders,
  );
  if (!assertOk("removeUsageApiKey", "POST /remove_usage_api_key", removeUsageKeyRes)) return;
  checkAndLog(removeUsageKeyRes.response, {
    "removeUsageApiKey success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  }, "removeUsageApiKey");

  // ── 20. removePkpFromGroup ────────────────────────────────────────────────
  const removePkpRes = client.removePkpFromGroup(
    { group_id: groupId, pkp_id: walletAddress },
    authHeaders,
  );
  if (!assertOk("removePkpFromGroup", "POST /remove_pkp_from_group", removePkpRes)) return;
  checkAndLog(removePkpRes.response, {
    "removePkpFromGroup success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  }, "removePkpFromGroup");

  // ── 21. removeActionFromGroup ─────────────────────────────────────────────
  const removeActionRes = client.removeActionFromGroup(
    { group_id: groupId, action_ipfs_cid: ipfsId },
    authHeaders,
  );
  assertOk("removeActionFromGroup", "POST /remove_action_from_group", removeActionRes);
  checkAndLog(removeActionRes.response, {
    "removeActionFromGroup success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  }, "removeActionFromGroup");
}