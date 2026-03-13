/**
 * Integration tests for lit-api-server.
 * Exercises every endpoint in the OpenAPI spec using a full account lifecycle.
 *
 * Usage:
 *   k6 run k6/integration.spec.ts
 *   BASE_URL=https://your-instance.phala.network/core/v1 k6 run k6/integration.spec.ts
 */
import { checkAndLog } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { createAccountAndUsageKey } from "../setup.ts";
import { assertOk } from "../helpers.ts";
import { HELLO_WORLD_CODE } from "../LitActionCode/index.ts";
import { BASE_URL } from "../defaults.ts";

export interface IntegrationSetupData {
  apiKey: string;
  walletAddress: string;
  usageApiKey: string;
}

export function setup(): IntegrationSetupData {
  const { apiKey, walletAddress, usageApiKey } = createAccountAndUsageKey({
    accountName: "k6-integration-test",
    accountDescription: "Integration test account",
    usageKeyName: "k6-integration-usage-key",
    usageKeyDescription: "Integration test usage key",
    setupContext: "integration",
  });
  return { apiKey, walletAddress, usageApiKey };
}

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

export default function (data: IntegrationSetupData) {
  const client = new LitApiServerClient({ baseUrl: BASE_URL });
  const authHeaders = { "X-Api-Key": data.apiKey };

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

  // ── 3. accountExists ──────────────────────────────────────────────────────
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

  // ── 4. createWallet ───────────────────────────────────────────────────────
  const createWalletRes = client.createWallet(authHeaders);
  if (!assertOk("createWallet", "GET /create_wallet", createWalletRes)) return;
  const walletData = createWalletRes.data as { wallet_address: string };
  checkAndLog(createWalletRes.response, {
    "createWallet returns wallet_address": () =>
      typeof walletData.wallet_address === "string" && walletData.wallet_address.length > 0,
  }, "createWallet");
  const walletAddress = walletData.wallet_address;

  // ── 5. listWallets ────────────────────────────────────────────────────────
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

  // ── 6. addGroup ───────────────────────────────────────────────────────────
  const addGroupRes = client.addGroup(
    {
      group_name: "k6-test-group",
      group_description: "Integration test group",
      pkp_ids_permitted: [],
      cid_hashes_permitted: [],
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

  // ── 7. listGroups — extract groupId for subsequent tests ──────────────────
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

  // ── 8. addActionToGroup ───────────────────────────────────────────────────
  const addActionRes = client.addActionToGroup(
    {
      group_id: parseInt(groupId),
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

  // ── 9. listActions ───────────────────────────────────────────────────────
  const listActionsRes = client.listActions(
    { group_id: parseInt(groupId), page_number: 0, page_size: 10 },
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

  // ── 10. addPkpToGroup ─────────────────────────────────────────────────────
  const addPkpRes = client.addPkpToGroup(
    { group_id: parseInt(groupId), pkp_id: walletAddress },
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

  // ── 11. listWalletsInGroup ────────────────────────────────────────────────
  const listWiGRes = client.listWalletsInGroup(
    { group_id: parseInt(groupId), page_number: 0, page_size: 10 },
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

  // ── 12. addUsageApiKey ────────────────────────────────────────────────────
  const addUsageKeyRes = client.addUsageApiKey(
    {
      name: "k6-usage-key",
      description: "Integration test usage key",
      can_create_groups: false,
      can_delete_groups: false,
      can_create_pkps: false,
      can_manage_ipfs_ids_in_groups: [],
      can_add_pkp_to_groups: [],
      can_remove_pkp_from_groups: [],
      can_execute_in_groups: [0],
    },
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
  const usageKeyHeaders = { "X-Api-Key": usageApiKey };

  // ── 13. litAction ─────────────────────────────────────────────────────────
  const litActionRes = client.litAction(
    { code: HELLO_WORLD_CODE, js_params: null },
    usageKeyHeaders,
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

  // ── 14. listApiKeys ───────────────────────────────────────────────────────
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

  // ── 15. updateGroup ───────────────────────────────────────────────────────
  const updateGroupRes = client.updateGroup(
    {
      group_id: parseInt(groupId),
      name: "k6-test-group-updated",
      description: "Updated integration test group",
      pkp_ids_permitted: [],
      cid_hashes_permitted: [],
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

  // ── 16. updateActionMetadata ──────────────────────────────────────────────
  const updateActionRes = client.updateActionMetadata(
    {
      group_id: parseInt(groupId),
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

  // ── 17. updateUsageApiKeyMetadata ─────────────────────────────────────────
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

  // ── 18. removeUsageApiKey ─────────────────────────────────────────────────
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

  // ── 19. removePkpFromGroup ────────────────────────────────────────────────
  const removePkpRes = client.removePkpFromGroup(
    { group_id: parseInt(groupId), pkp_id: walletAddress },
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

  // ── 20. removeActionFromGroup ─────────────────────────────────────────────
  const removeActionRes = client.removeActionFromGroup(
    { group_id: parseInt(groupId) , action_ipfs_cid: ipfsId },
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