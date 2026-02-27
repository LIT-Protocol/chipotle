import { checkAndLog } from "./check.ts";
import { LitApiServerClient } from "./litApiServer.ts";

const baseUrl =
  __ENV.BASE_URL ||
  "https://e364da71b0c9af3b9068daa6321edd6ee932aa89-8000.dstack-pha-prod5.phala.network/core/v1";
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
    page_number: "oh",
    page_size: "kit",
  };
  headers = {
    "X-Api-Key": "even",
  };

  const listApiKeysResponseData = litApiServerClient.listApiKeys(
    params,
    headers,
  );
  checkAndLog(listApiKeysResponseData.response, {
    "listApiKeys 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "listApiKeys");

  /**
   *
   */
  newAccountRequest = {
    account_name: "arrange",
    account_description: "fat",
    initial_balance: "1000000",
  };

  const newAccountResponseData =
    litApiServerClient.newAccount(newAccountRequest);
  checkAndLog(newAccountResponseData.response, {
    "newAccount 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "newAccount");

  /**
   *
   */
  headers = {
    "X-Api-Key": "provided",
  };

  const accountExistsResponseData = litApiServerClient.accountExists(headers);
  checkAndLog(accountExistsResponseData.response, {
    "accountExists 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "accountExists");

  /**
   *
   */
  headers = {
    "X-Api-Key": "thread",
  };

  const createWalletResponseData = litApiServerClient.createWallet(headers);
  checkAndLog(createWalletResponseData.response, {
    "createWallet 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "createWallet");

  /**
   *
   */
  litActionRequest = {
    code: "grizzled",
    js_params: undefined,
  };
  headers = {
    "X-Api-Key": "for",
  };

  const litActionResponseData = litApiServerClient.litAction(
    litActionRequest,
    headers,
  );
  checkAndLog(litActionResponseData.response, {
    "litAction 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "litAction");

  /**
   *
   */
  code = "off";

  const getLitActionIpfsIdResponseData =
    litApiServerClient.getLitActionIpfsId(code);
  checkAndLog(getLitActionIpfsIdResponseData.response, {
    "getLitActionIpfsId 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "getLitActionIpfsId");

  /**
   *
   */
  addGroupRequest = {
    group_name: "any",
    group_description: "absent",
    permitted_actions: [],
    pkps: [],
    all_wallets_permitted: false,
    all_actions_permitted: true,
  };
  headers = {
    "X-Api-Key": "however",
  };

  const addGroupResponseData = litApiServerClient.addGroup(
    addGroupRequest,
    headers,
  );
  checkAndLog(addGroupResponseData.response, {
    "addGroup 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "addGroup");

  /**
   *
   */
  addActionToGroupRequest = {
    group_id: "recovery",
    action_ipfs_cid: "beautifully",
    name: "stealthily",
    description: "hm",
  };
  headers = {
    "X-Api-Key": "memorise",
  };

  const addActionToGroupResponseData = litApiServerClient.addActionToGroup(
    addActionToGroupRequest,
    headers,
  );
  checkAndLog(addActionToGroupResponseData.response, {
    "addActionToGroup 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "addActionToGroup");

  /**
   *
   */
  addPkpToGroupRequest = {
    group_id: "dissemble",
    pkp_public_key: "greedy",
  };
  headers = {
    "X-Api-Key": "pastel",
  };

  const addPkpToGroupResponseData = litApiServerClient.addPkpToGroup(
    addPkpToGroupRequest,
    headers,
  );
  checkAndLog(addPkpToGroupResponseData.response, {
    "addPkpToGroup 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "addPkpToGroup");

  /**
   *
   */
  removePkpFromGroupRequest = {
    group_id: "grouper",
    pkp_public_key: "yowza",
  };
  headers = {
    "X-Api-Key": "abaft",
  };

  const removePkpFromGroupResponseData = litApiServerClient.removePkpFromGroup(
    removePkpFromGroupRequest,
    headers,
  );
  checkAndLog(removePkpFromGroupResponseData.response, {
    "removePkpFromGroup 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "removePkpFromGroup");

  /**
   *
   */
  addUsageApiKeyRequest = {
    expiration: "boo",
    balance: "decide",
  };
  headers = {
    "X-Api-Key": "pace",
  };

  const addUsageApiKeyResponseData = litApiServerClient.addUsageApiKey(
    addUsageApiKeyRequest,
    headers,
  );
  checkAndLog(addUsageApiKeyResponseData.response, {
    "addUsageApiKey 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "addUsageApiKey");

  /**
   *
   */
  removeUsageApiKeyRequest = {
    usage_api_key: "creamy",
  };
  headers = {
    "X-Api-Key": "yet",
  };

  const removeUsageApiKeyResponseData = litApiServerClient.removeUsageApiKey(
    removeUsageApiKeyRequest,
    headers,
  );
  checkAndLog(removeUsageApiKeyResponseData.response, {
    "removeUsageApiKey 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "removeUsageApiKey");

  /**
   *
   */
  updateGroupRequest = {
    group_id: "yet",
    name: "where",
    description: "unimpressively",
    all_wallets_permitted: true,
    all_actions_permitted: true,
  };
  headers = {
    "X-Api-Key": "abaft",
  };

  const updateGroupResponseData = litApiServerClient.updateGroup(
    updateGroupRequest,
    headers,
  );
  checkAndLog(updateGroupResponseData.response, {
    "updateGroup 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "updateGroup");

  /**
   *
   */
  removeActionFromGroupRequest = {
    group_id: "hmph",
    action_ipfs_cid: "treasure",
  };
  headers = {
    "X-Api-Key": "aha",
  };

  const removeActionFromGroupResponseData =
    litApiServerClient.removeActionFromGroup(
      removeActionFromGroupRequest,
      headers,
    );
  checkAndLog(removeActionFromGroupResponseData.response, {
    "removeActionFromGroup 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "removeActionFromGroup");

  /**
   *
   */
  updateActionMetadataRequest = {
    group_id: "following",
    action_ipfs_cid: "sizzling",
    name: "unpleasant",
    description: "decryption",
  };
  headers = {
    "X-Api-Key": "truthfully",
  };

  const updateActionMetadataResponseData =
    litApiServerClient.updateActionMetadata(
      updateActionMetadataRequest,
      headers,
    );
  checkAndLog(updateActionMetadataResponseData.response, {
    "updateActionMetadata 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "updateActionMetadata");

  /**
   *
   */
  updateUsageApiKeyMetadataRequest = {
    usage_api_key: "frizz",
    name: "sheathe",
    description: "around",
  };
  headers = {
    "X-Api-Key": "deeply",
  };

  const updateUsageApiKeyMetadataResponseData =
    litApiServerClient.updateUsageApiKeyMetadata(
      updateUsageApiKeyMetadataRequest,
      headers,
    );
  checkAndLog(updateUsageApiKeyMetadataResponseData.response, {
    "updateUsageApiKeyMetadata 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "updateUsageApiKeyMetadata");

  /**
   *
   */
  params = {
    page_number: "furiously",
    page_size: "absentmindedly",
  };
  headers = {
    "X-Api-Key": "linseed",
  };

  const listGroupsResponseData = litApiServerClient.listGroups(params, headers);
  checkAndLog(listGroupsResponseData.response, {
    "listGroups 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "listGroups");

  /**
   *
   */
  params = {
    page_number: "hence",
    page_size: "sell",
  };
  headers = {
    "X-Api-Key": "what",
  };

  const listWalletsResponseData = litApiServerClient.listWallets(
    params,
    headers,
  );
  checkAndLog(listWalletsResponseData.response, {
    "listWallets 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "listWallets");

  /**
   *
   */
  params = {
    group_id: "dependency",
    page_number: "even",
    page_size: "onto",
  };
  headers = {
    "X-Api-Key": "naughty",
  };

  const listWalletsInGroupResponseData = litApiServerClient.listWalletsInGroup(
    params,
    headers,
  );
  checkAndLog(listWalletsInGroupResponseData.response, {
    "listWalletsInGroup 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "listWalletsInGroup");

  /**
   *
   */
  params = {
    group_id: "but",
    page_number: "er",
    page_size: "blowgun",
  };
  headers = {
    "X-Api-Key": "yet",
  };

  const listActionsResponseData = litApiServerClient.listActions(
    params,
    headers,
  );
  checkAndLog(listActionsResponseData.response, {
    "listActions 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "listActions");

  /**
   *
   */
  const getNodeChainConfigResponseData =
    litApiServerClient.getNodeChainConfig();
  checkAndLog(getNodeChainConfigResponseData.response, {
    "getNodeChainConfig 2xx": (r) => (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, "getNodeChainConfig");
}
