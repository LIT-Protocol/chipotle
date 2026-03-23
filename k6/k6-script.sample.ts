import { LitApiServerClient } from "./litApiServer.ts";

const baseUrl = "<BASE_URL>";
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
    page_number: "saturate",
    page_size: "reporter",
  };
  headers = {
    "X-Api-Key": "than",
  };

  const listApiKeysResponseData = litApiServerClient.listApiKeys(
    params,
    headers,
  );

  /**
   *
   */
  newAccountRequest = {
    account_name: "bolster",
    account_description: "ick",
  };

  const newAccountResponseData =
    litApiServerClient.newAccount(newAccountRequest);

  /**
   *
   */
  headers = {
    "X-Api-Key": "yippee",
  };

  const accountExistsResponseData = litApiServerClient.accountExists(headers);

  /**
   *
   */
  headers = {
    "X-Api-Key": "materialise",
  };

  const createWalletResponseData = litApiServerClient.createWallet(headers);

  /**
   *
   */
  litActionRequest = {
    code: "into",
    js_params: undefined,
  };
  headers = {
    "X-Api-Key": "than",
  };

  const litActionResponseData = litApiServerClient.litAction(
    litActionRequest,
    headers,
  );

  /**
   *
   */
  code = "careless";

  const getLitActionIpfsIdResponseData =
    litApiServerClient.getLitActionIpfsId(code);

  /**
   *
   */
  addGroupRequest = {
    group_name: "after",
    group_description: "almost",
    permitted_actions: [],
    pkps: [],
    all_wallets_permitted: false,
    all_actions_permitted: false,
  };
  headers = {
    "X-Api-Key": "duh",
  };

  const addGroupResponseData = litApiServerClient.addGroup(
    addGroupRequest,
    headers,
  );

  /**
   *
   */
  addActionToGroupRequest = {
    group_id: "furthermore",
    action_ipfs_cid: "although",
  };
  headers = {
    "X-Api-Key": "whether",
  };

  const addActionToGroupResponseData = litApiServerClient.addActionToGroup(
    addActionToGroupRequest,
    headers,
  );

  /**
   *
   */
  addPkpToGroupRequest = {
    group_id: "all",
    pkp_id: "beyond",
  };
  headers = {
    "X-Api-Key": "psst",
  };

  const addPkpToGroupResponseData = litApiServerClient.addPkpToGroup(
    addPkpToGroupRequest,
    headers,
  );

  /**
   *
   */
  removePkpFromGroupRequest = {
    group_id: "usually",
    pkp_id: "atop",
  };
  headers = {
    "X-Api-Key": "who",
  };

  const removePkpFromGroupResponseData = litApiServerClient.removePkpFromGroup(
    removePkpFromGroupRequest,
    headers,
  );

  /**
   *
   */
  addUsageApiKeyRequest = {
    expiration: "whopping",
    balance: "likely",
    name: "wetly",
    description: "aha",
  };
  headers = {
    "X-Api-Key": "whimsical",
  };

  const addUsageApiKeyResponseData = litApiServerClient.addUsageApiKey(
    addUsageApiKeyRequest,
    headers,
  );

  /**
   *
   */
  removeUsageApiKeyRequest = {
    usage_api_key: "whoever",
  };
  headers = {
    "X-Api-Key": "minister",
  };

  const removeUsageApiKeyResponseData = litApiServerClient.removeUsageApiKey(
    removeUsageApiKeyRequest,
    headers,
  );

  /**
   *
   */
  updateGroupRequest = {
    group_id: "ick",
    name: "good-natured",
    description: "phew",
    all_wallets_permitted: false,
    all_actions_permitted: true,
  };
  headers = {
    "X-Api-Key": "than",
  };

  const updateGroupResponseData = litApiServerClient.updateGroup(
    updateGroupRequest,
    headers,
  );

  /**
   *
   */
  removeActionFromGroupRequest = {
    group_id: "jaywalk",
    action_ipfs_cid: "vice",
  };
  headers = {
    "X-Api-Key": "after",
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
    group_id: "outside",
    action_ipfs_cid: "along",
    name: "consequently",
    description: "moor",
  };
  headers = {
    "X-Api-Key": "apropos",
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
    usage_api_key: "other",
    name: "broken",
    description: "capitalize",
  };
  headers = {
    "X-Api-Key": "scram",
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
    page_number: "whenever",
    page_size: "pillbox",
  };
  headers = {
    "X-Api-Key": "pro",
  };

  const listGroupsResponseData = litApiServerClient.listGroups(params, headers);

  /**
   *
   */
  params = {
    page_number: "marvelous",
    page_size: "roger",
  };
  headers = {
    "X-Api-Key": "strictly",
  };

  const listWalletsResponseData = litApiServerClient.listWallets(
    params,
    headers,
  );

  /**
   *
   */
  params = {
    group_id: "elver",
    page_number: "even",
    page_size: "carpool",
  };
  headers = {
    "X-Api-Key": "flashy",
  };

  const listWalletsInGroupResponseData = litApiServerClient.listWalletsInGroup(
    params,
    headers,
  );

  /**
   *
   */
  params = {
    group_id: "phew",
    page_number: "amidst",
    page_size: "severe",
  };
  headers = {
    "X-Api-Key": "over",
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

  /**
   *
   */

  const getApiPayersResponseData = litApiServerClient.getApiPayers();

  /**
   *
   */

  const getAdminApiPayerResponseData = litApiServerClient.getAdminApiPayer();
}
