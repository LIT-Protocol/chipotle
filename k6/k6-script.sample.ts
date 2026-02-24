import { LitApiServerClient } from "./litApiServer.ts";

const baseUrl =
  __ENV.BASE_URL ||
  "https://36da669c852c9bd4fdea27dd331c07ff776bd125-8000.dstack-pha-prod5.phala.network/core/v1";
const litApiServerClient = new LitApiServerClient({ baseUrl });

export default function () {
  let params,
    headers,
    newAccountRequest,
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
    page_number: "before",
    page_size: "giant",
  };
  headers = {
    "X-Api-Key": "reproachfully",
  };

  const listApiKeysResponseData = litApiServerClient.listApiKeys(
    params,
    headers,
  );

  /**
   *
   */
  newAccountRequest = {
    account_name: "in",
    account_description: "um",
    initial_balance: "more",
  };

  const newAccountResponseData =
    litApiServerClient.newAccount(newAccountRequest);

  /**
   *
   */
  headers = {
    "X-Api-Key": "surface",
  };

  const accountExistsResponseData = litApiServerClient.accountExists(headers);

  /**
   *
   */
  headers = {
    "X-Api-Key": "even",
  };

  const createWalletResponseData = litApiServerClient.createWallet(headers);

  /**
   *
   */
  litActionRequest = {
    code: "inside",
    js_params: undefined,
  };
  headers = {
    "X-Api-Key": "trolley",
  };

  const litActionResponseData = litApiServerClient.litAction(
    litActionRequest,
    headers,
  );

  /**
   *
   */
  code = "wee";

  const getLitActionIpfsIdResponseData =
    litApiServerClient.getLitActionIpfsId(code);

  /**
   *
   */
  addGroupRequest = {
    group_name: "athletic",
    group_description: "drat",
    permitted_actions: [],
    pkps: [],
    all_wallets_permitted: true,
    all_actions_permitted: false,
  };
  headers = {
    "X-Api-Key": "pish",
  };

  const addGroupResponseData = litApiServerClient.addGroup(
    addGroupRequest,
    headers,
  );

  /**
   *
   */
  addActionToGroupRequest = {
    group_id: "season",
    action_ipfs_cid: "mismatch",
    name: "question",
    description: "inscribe",
  };
  headers = {
    "X-Api-Key": "zowie",
  };

  const addActionToGroupResponseData = litApiServerClient.addActionToGroup(
    addActionToGroupRequest,
    headers,
  );

  /**
   *
   */
  addPkpToGroupRequest = {
    group_id: "hover",
    pkp_public_key: "suspiciously",
  };
  headers = {
    "X-Api-Key": "breakable",
  };

  const addPkpToGroupResponseData = litApiServerClient.addPkpToGroup(
    addPkpToGroupRequest,
    headers,
  );

  /**
   *
   */
  removePkpFromGroupRequest = {
    group_id: "uselessly",
    pkp_public_key: "keel",
  };
  headers = {
    "X-Api-Key": "provided",
  };

  const removePkpFromGroupResponseData = litApiServerClient.removePkpFromGroup(
    removePkpFromGroupRequest,
    headers,
  );

  /**
   *
   */
  addUsageApiKeyRequest = {
    expiration: "under",
    balance: "abscond",
  };
  headers = {
    "X-Api-Key": "woot",
  };

  const addUsageApiKeyResponseData = litApiServerClient.addUsageApiKey(
    addUsageApiKeyRequest,
    headers,
  );

  /**
   *
   */
  removeUsageApiKeyRequest = {
    usage_api_key: "approximate",
  };
  headers = {
    "X-Api-Key": "unlike",
  };

  const removeUsageApiKeyResponseData = litApiServerClient.removeUsageApiKey(
    removeUsageApiKeyRequest,
    headers,
  );

  /**
   *
   */
  updateGroupRequest = {
    group_id: "furthermore",
    name: "wilderness",
    description: "inculcate",
    all_wallets_permitted: false,
    all_actions_permitted: true,
  };
  headers = {
    "X-Api-Key": "triumphantly",
  };

  const updateGroupResponseData = litApiServerClient.updateGroup(
    updateGroupRequest,
    headers,
  );

  /**
   *
   */
  removeActionFromGroupRequest = {
    group_id: "fork",
    action_ipfs_cid: "fill",
  };
  headers = {
    "X-Api-Key": "unconscious",
  };

  const removeActionFromGroupResponseData =
    litApiServerClient.removeActionFromGroup(
      removeActionFromGroupRequest,
      headers,
    );

  /**
   *
   */
  updateActionMetadataRequest = {
    group_id: "igloo",
    action_ipfs_cid: "unnaturally",
    name: "instantly",
    description: "strictly",
  };
  headers = {
    "X-Api-Key": "yuck",
  };

  const updateActionMetadataResponseData =
    litApiServerClient.updateActionMetadata(
      updateActionMetadataRequest,
      headers,
    );

  /**
   *
   */
  updateUsageApiKeyMetadataRequest = {
    usage_api_key: "once",
    name: "generally",
    description: "sardonic",
  };
  headers = {
    "X-Api-Key": "overwork",
  };

  const updateUsageApiKeyMetadataResponseData =
    litApiServerClient.updateUsageApiKeyMetadata(
      updateUsageApiKeyMetadataRequest,
      headers,
    );

  /**
   *
   */
  params = {
    page_number: "astride",
    page_size: "inside",
  };
  headers = {
    "X-Api-Key": "pave",
  };

  const listGroupsResponseData = litApiServerClient.listGroups(params, headers);

  /**
   *
   */
  params = {
    page_number: "considering",
    page_size: "teeming",
  };
  headers = {
    "X-Api-Key": "cap",
  };

  const listWalletsResponseData = litApiServerClient.listWallets(
    params,
    headers,
  );

  /**
   *
   */
  params = {
    group_id: "paralyse",
    page_number: "about",
    page_size: "irk",
  };
  headers = {
    "X-Api-Key": "indeed",
  };

  const listWalletsInGroupResponseData = litApiServerClient.listWalletsInGroup(
    params,
    headers,
  );

  /**
   *
   */
  params = {
    group_id: "crossly",
    page_number: "who",
    page_size: "consequently",
  };
  headers = {
    "X-Api-Key": "beyond",
  };

  const listActionsResponseData = litApiServerClient.listActions(
    params,
    headers,
  );

  /**
   *
   */

  const getNodeChainConfigResponseData =
    litApiServerClient.getNodeChainConfig();
}
