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
    page_number: "monthly",
    page_size: "scar",
  };
  headers = {
    "X-Api-Key": "detective",
  };

  const listApiKeysResponseData = litApiServerClient.listApiKeys(
    params,
    headers,
  );

  /**
   *
   */
  newAccountRequest = {
    account_name: "after",
    account_description: "anxiously",
  };

  const newAccountResponseData =
    litApiServerClient.newAccount(newAccountRequest);

  /**
   *
   */
  headers = {
    "X-Api-Key": "daintily",
  };

  const accountExistsResponseData = litApiServerClient.accountExists(headers);

  /**
   *
   */
  headers = {
    "X-Api-Key": "inveigle",
  };

  const createWalletResponseData = litApiServerClient.createWallet(headers);

  /**
   *
   */
  litActionRequest = {
    code: "woot",
    js_params: undefined,
  };
  headers = {
    "X-Api-Key": "quintuple",
  };

  const litActionResponseData = litApiServerClient.litAction(
    litActionRequest,
    headers,
  );

  /**
   *
   */
  code = "industrialize";

  const getLitActionIpfsIdResponseData =
    litApiServerClient.getLitActionIpfsId(code);

  /**
   *
   */
  addGroupRequest = {
    group_name: "little",
    group_description: "wobbly",
    permitted_actions: [],
    pkps: [],
    all_wallets_permitted: false,
    all_actions_permitted: true,
  };
  headers = {
    "X-Api-Key": "meanwhile",
  };

  const addGroupResponseData = litApiServerClient.addGroup(
    addGroupRequest,
    headers,
  );

  /**
   *
   */
  addActionToGroupRequest = {
    group_id: "possession",
    action_ipfs_cid: "fatally",
    name: "while",
    description: "kiddingly",
  };
  headers = {
    "X-Api-Key": "every",
  };

  const addActionToGroupResponseData = litApiServerClient.addActionToGroup(
    addActionToGroupRequest,
    headers,
  );

  /**
   *
   */
  addPkpToGroupRequest = {
    group_id: "aha",
    pkp_id: "despite",
  };
  headers = {
    "X-Api-Key": "aside",
  };

  const addPkpToGroupResponseData = litApiServerClient.addPkpToGroup(
    addPkpToGroupRequest,
    headers,
  );

  /**
   *
   */
  removePkpFromGroupRequest = {
    group_id: "narrowcast",
    pkp_id: "silver",
  };
  headers = {
    "X-Api-Key": "lively",
  };

  const removePkpFromGroupResponseData = litApiServerClient.removePkpFromGroup(
    removePkpFromGroupRequest,
    headers,
  );

  /**
   *
   */
  addUsageApiKeyRequest = {
    expiration: "worst",
    balance: "clinking",
    name: "ignorant",
    description: "overload",
  };
  headers = {
    "X-Api-Key": "who",
  };

  const addUsageApiKeyResponseData = litApiServerClient.addUsageApiKey(
    addUsageApiKeyRequest,
    headers,
  );

  /**
   *
   */
  removeUsageApiKeyRequest = {
    usage_api_key: "interesting",
  };
  headers = {
    "X-Api-Key": "uh-huh",
  };

  const removeUsageApiKeyResponseData = litApiServerClient.removeUsageApiKey(
    removeUsageApiKeyRequest,
    headers,
  );

  /**
   *
   */
  updateGroupRequest = {
    group_id: "intent",
    name: "whoa",
    description: "depart",
    all_wallets_permitted: true,
    all_actions_permitted: true,
  };
  headers = {
    "X-Api-Key": "yahoo",
  };

  const updateGroupResponseData = litApiServerClient.updateGroup(
    updateGroupRequest,
    headers,
  );

  /**
   *
   */
  removeActionFromGroupRequest = {
    group_id: "pneumonia",
    action_ipfs_cid: "shiny",
  };
  headers = {
    "X-Api-Key": "content",
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
    group_id: "term",
    action_ipfs_cid: "stealthily",
    name: "utterly",
    description: "ack",
  };
  headers = {
    "X-Api-Key": "fellow",
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
    usage_api_key: "conversation",
    name: "fooey",
    description: "clear-cut",
  };
  headers = {
    "X-Api-Key": "even",
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
    page_number: "patiently",
    page_size: "powerless",
  };
  headers = {
    "X-Api-Key": "nasalise",
  };

  const listGroupsResponseData = litApiServerClient.listGroups(params, headers);

  /**
   *
   */
  params = {
    page_number: "provided",
    page_size: "during",
  };
  headers = {
    "X-Api-Key": "clamp",
  };

  const listWalletsResponseData = litApiServerClient.listWallets(
    params,
    headers,
  );

  /**
   *
   */
  params = {
    group_id: "duh",
    page_number: "merrily",
    page_size: "querulous",
  };
  headers = {
    "X-Api-Key": "putrid",
  };

  const listWalletsInGroupResponseData = litApiServerClient.listWalletsInGroup(
    params,
    headers,
  );

  /**
   *
   */
  params = {
    group_id: "courageously",
    page_number: "pave",
    page_size: "knavishly",
  };
  headers = {
    "X-Api-Key": "forearm",
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
