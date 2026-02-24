import { check } from "k6";
import type { Response } from "k6/http";
import { LitApiServerClient } from "./litApiServer.ts";

const baseUrl =
  __ENV.BASE_URL ||
  "https://36da669c852c9bd4fdea27dd331c07ff776bd125-8000.dstack-pha-prod5.phala.network/core/v1";
const litApiServerClient = new LitApiServerClient({ baseUrl });

function assertOk(name: string, endpoint: string, res: { response: Response }) {
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
        msg = body.message ?? body.error ?? body.detail ?? (typeof body === "string" ? body : JSON.stringify(body));
      } catch {
        msg = (response.body as string) || "(no body)";
      }
    }
    console.error(`FAIL ${name} | ${endpoint} | ${status} | ${msg}`);
  }
  check(response, { [`${name} 2xx`]: (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300 });
}

export default function () {
  let params,
    newAccountRequest,
    apiKey,
    litActionRequest,
    code,
    addGroupRequest,
    addActionToGroupRequest,
    addPkpToGroupRequest,
    removePkpFromGroupRequest,
    addUsageApiKeyRequest,
    removeUsageApiKeyRequest,
    updateGroupRequest,
    removeActionFromGroupRequest,
    updateActionMetadataRequest,
    updateUsageApiKeyMetadataRequest;

  /**
   *
   */
  params = {
    api_key: "vain",
    page_number: "meatloaf",
    page_size: "embed",
  };

  const listApiKeysResponseData = litApiServerClient.listApiKeys(params);
  assertOk("listApiKeys", "GET /list_api_keys", listApiKeysResponseData);

  /**
   *
   */
  newAccountRequest = {
    account_name: "any",
    account_description: "duster",
    initial_balance: "insignificant",
  };

  const newAccountResponseData =
    litApiServerClient.newAccount(newAccountRequest);
  assertOk("newAccount", "POST /new_account", newAccountResponseData);

  /**
   *
   */
  apiKey = "deadly";

  const accountExistsResponseData = litApiServerClient.accountExists(apiKey);
  assertOk("accountExists", "GET /account_exists/{api_key}", accountExistsResponseData);

  /**
   *
   */
  apiKey = "blight";

  const createWalletResponseData = litApiServerClient.createWallet(apiKey);
  assertOk("createWallet", "GET /create_wallet/{api_key}", createWalletResponseData);

  /**
   *
   */
  litActionRequest = {
    api_key: "psst",
    code: "plus",
    js_params: undefined,
  };

  const litActionResponseData = litApiServerClient.litAction(litActionRequest);
  assertOk("litAction", "POST /lit_action", litActionResponseData);

  /**
   *
   */
  code = "except";

  const getLitActionIpfsIdResponseData =
    litApiServerClient.getLitActionIpfsId(code);
  assertOk("getLitActionIpfsId", "GET /get_lit_action_ipfs_id/{code}", getLitActionIpfsIdResponseData);

  /**
   *
   */
  addGroupRequest = {
    api_key: "ugh",
    group_name: "follower",
    group_description: "cinder",
    permitted_actions: [],
    pkps: [],
    all_wallets_permitted: true,
    all_actions_permitted: false,
  };

  const addGroupResponseData = litApiServerClient.addGroup(addGroupRequest);
  assertOk("addGroup", "POST /add_group", addGroupResponseData);

  /**
   *
   */
  addActionToGroupRequest = {
    api_key: "tomorrow",
    group_id: "reproach",
    action_ipfs_cid: "opposite",
    name: "though",
    description: "tidy",
  };

  const addActionToGroupResponseData = litApiServerClient.addActionToGroup(
    addActionToGroupRequest,
  );
  assertOk("addActionToGroup", "POST /add_action_to_group", addActionToGroupResponseData);

  /**
   *
   */
  addPkpToGroupRequest = {
    api_key: "fray",
    group_id: "where",
    pkp_public_key: "while",
  };

  const addPkpToGroupResponseData =
    litApiServerClient.addPkpToGroup(addPkpToGroupRequest);
  assertOk("addPkpToGroup", "POST /add_pkp_to_group", addPkpToGroupResponseData);

  /**
   *
   */
  removePkpFromGroupRequest = {
    api_key: "around",
    group_id: "till",
    pkp_public_key: "ethyl",
  };

  const removePkpFromGroupResponseData = litApiServerClient.removePkpFromGroup(
    removePkpFromGroupRequest,
  );
  assertOk("removePkpFromGroup", "POST /remove_pkp_from_group", removePkpFromGroupResponseData);

  /**
   *
   */
  addUsageApiKeyRequest = {
    api_key: "and",
    expiration: "because",
    balance: "amnesty",
  };

  const addUsageApiKeyResponseData = litApiServerClient.addUsageApiKey(
    addUsageApiKeyRequest,
  );
  assertOk("addUsageApiKey", "POST /add_usage_api_key", addUsageApiKeyResponseData);

  /**
   *
   */
  removeUsageApiKeyRequest = {
    api_key: "catalyze",
    usage_api_key: "leading",
  };

  const removeUsageApiKeyResponseData = litApiServerClient.removeUsageApiKey(
    removeUsageApiKeyRequest,
  );
  assertOk("removeUsageApiKey", "POST /remove_usage_api_key", removeUsageApiKeyResponseData);

  /**
   *
   */
  updateGroupRequest = {
    api_key: "who",
    group_id: "yowza",
    name: "meh",
    description: "readily",
    all_wallets_permitted: true,
    all_actions_permitted: true,
  };

  const updateGroupResponseData =
    litApiServerClient.updateGroup(updateGroupRequest);
  assertOk("updateGroup", "POST /update_group", updateGroupResponseData);

  /**
   *
   */
  removeActionFromGroupRequest = {
    api_key: "until",
    group_id: "for",
    action_ipfs_cid: "powerfully",
  };

  const removeActionFromGroupResponseData =
    litApiServerClient.removeActionFromGroup(removeActionFromGroupRequest);
  assertOk("removeActionFromGroup", "POST /remove_action_from_group", removeActionFromGroupResponseData);

  /**
   *
   */
  updateActionMetadataRequest = {
    api_key: "although",
    group_id: "obstruct",
    action_ipfs_cid: "phooey",
    name: "sediment",
    description: "knavishly",
  };

  const updateActionMetadataResponseData =
    litApiServerClient.updateActionMetadata(updateActionMetadataRequest);
  assertOk("updateActionMetadata", "POST /update_action_metadata", updateActionMetadataResponseData);

  /**
   *
   */
  updateUsageApiKeyMetadataRequest = {
    api_key: "lazily",
    usage_api_key: "because",
    name: "navigate",
    description: "cemetery",
  };

  const updateUsageApiKeyMetadataResponseData =
    litApiServerClient.updateUsageApiKeyMetadata(
      updateUsageApiKeyMetadataRequest,
    );
  assertOk("updateUsageApiKeyMetadata", "POST /update_usage_api_key_metadata", updateUsageApiKeyMetadataResponseData);

  /**
   *
   */
  params = {
    api_key: "acclaimed",
    page_number: "jury",
    page_size: "scratchy",
  };

  const listGroupsResponseData = litApiServerClient.listGroups(params);
  assertOk("listGroups", "GET /list_groups", listGroupsResponseData);

  /**
   *
   */
  params = {
    api_key: "as",
    page_number: "pro",
    page_size: "lazily",
  };

  const listWalletsResponseData = litApiServerClient.listWallets(params);
  assertOk("listWallets", "GET /list_wallets", listWalletsResponseData);

  /**
   *
   */
  params = {
    api_key: "to",
    group_id: "as",
    page_number: "runway",
    page_size: "by",
  };

  const listWalletsInGroupResponseData =
    litApiServerClient.listWalletsInGroup(params);
  assertOk("listWalletsInGroup", "GET /list_wallets_in_group", listWalletsInGroupResponseData);

  /**
   *
   */
  params = {
    api_key: "once",
    group_id: "bin",
    page_number: "defendant",
    page_size: "however",
  };

  const listActionsResponseData = litApiServerClient.listActions(params);
  assertOk("listActions", "GET /list_actions", listActionsResponseData);

  /**
   *
   */
  const getNodeChainConfigResponseData =
    litApiServerClient.getNodeChainConfig();
  assertOk("getNodeChainConfig", "GET /get_node_chain_config", getNodeChainConfigResponseData);
}
