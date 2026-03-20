/**
 * Lit Node Simple API - JavaScript Core SDK
 *
 * Wrapper for the v1 API endpoints defined in lit-api-server.
 * Types match core/v1/models (request.rs, response.rs) and core/v1/endpoints.rs.
 * Routes are mounted at /core/v1/ (see src/main.rs).
 */

// --- Request types (match core/v1/models/request.rs) ---

/**
 * @typedef {Object} NewAccountOptions
 * @property {string} accountName - Name for the account
 * @property {string} accountDescription - Description for the account
 * @property {string} [email] - Optional email address for Stripe billing
 */

/**
 * @typedef {Object} LitActionOptions
 * @property {string} apiKey - Hex-encoded API key (from getApiKey)
 * @property {string} code - Lit action JavaScript code
 * @property {Object} [jsParams] - Optional JSON params passed to the lit action
 */

/**
 * @typedef {Object} AddGroupOptions
 * @property {string} apiKey - Account API key
 * @property {string} groupName - Name of the group (Group.metadata.name in AccountConfig.sol)
 * @property {string} [groupDescription=''] - Description of the group (Group.metadata.description in AccountConfig.sol)
 * @property {string[]} [pkpIdsPermitted=[]] - PKP IDs permitted to use the group (AccountConfig.sol Group.pkpId)
 * @property {string[]} [cidHashesPermitted=[]] - Keccak256 hashes of permitted action IPFS CIDs (AccountConfig.sol Group.cidHash)
 */

/**
 * @typedef {Object} AddActionOptions
 * @property {string} apiKey - Account API key
 * @property {string} name - Name for the action (stored in contract actionMetadata)
 * @property {string} description - Description for the action
 */

/**
 * @typedef {Object} AddActionToGroupOptions
 * @property {string} apiKey - Account API key
 * @property {string} groupId - Group ID (decimal or hex string)
 * @property {string} actionIpfsCid - IPFS CID for the action (keccak256-hashed on server)
 */

/**
 * @typedef {Object} AddPkpToGroupOptions
 * @property {string} apiKey - Account API key
 * @property {string} groupId - Group ID (decimal or hex string)
 * @property {string} pkpId - PKP ID
 */

/**
 * @typedef {Object} RemovePkpFromGroupOptions
 * @property {string} apiKey - Account API key
 * @property {string} groupId - Group ID (decimal or hex string)
 * @property {string} pkpId - PKP ID (must match value used when adding)
 */

/**
 * @typedef {Object} AddUsageApiKeyOptions
 * @property {string} apiKey - Account API key
 * @property {string} name - Name
 * @property {string} description - Description
 * @property {boolean} [canCreateGroups=false] - Permission to create groups
 * @property {boolean} [canDeleteGroups=false] - Permission to delete groups
 * @property {boolean} [canCreatePkps=false] - Permission to create PKPs
 * @property {number[]} [manageIpfsIdsInGroups=[]] - Group IDs to grant manage-IPFS-IDs permission (0 = all groups)
 * @property {number[]} [addPkpToGroups=[]] - Group IDs to grant add-PKP permission (0 = all groups)
 * @property {number[]} [removePkpFromGroups=[]] - Group IDs to grant remove-PKP permission (0 = all groups)
 * @property {number[]} [executeInGroups=[]] - Group IDs to grant execute permission (0 = all groups)
 */

/**
 * @typedef {Object} UpdateUsageApiKeyOptions
 * @property {string} apiKey - Account API key
 * @property {string} usageApiKey - The existing usage API key to update
 * @property {string} name - Name
 * @property {string} description - Description
 * @property {boolean} [canCreateGroups=false]
 * @property {boolean} [canDeleteGroups=false]
 * @property {boolean} [canCreatePkps=false]
 * @property {number[]} [manageIpfsIdsInGroups=[]]
 * @property {number[]} [addPkpToGroups=[]]
 * @property {number[]} [removePkpFromGroups=[]]
 * @property {number[]} [executeInGroups=[]]
 */

/**
 * @typedef {Object} RemoveUsageApiKeyOptions
 * @property {string} apiKey - Account API key
 * @property {string} usageApiKey - Usage API key to remove
 */

/**
 * @typedef {Object} UpdateGroupOptions
 * @property {string} apiKey - Account API key
 * @property {string} groupId - Group ID (decimal or hex string)
 * @property {string} name - Group name
 * @property {string} description - Group description
 * @property {string[]} [pkpIdsPermitted=[]] - PKP IDs permitted to use the group
 * @property {string[]} [cidHashesPermitted=[]] - Keccak256 hashes of permitted action IPFS CIDs
 */

/**
 * @typedef {Object} RemoveActionFromGroupOptions
 * @property {string} apiKey - Account API key
 * @property {string} groupId - Group ID (decimal or hex string)
 * @property {string} actionIpfsCid - IPFS CID for the action (keccak256-hashed on server)
 */

/**
 * @typedef {Object} UpdateActionMetadataOptions
 * @property {string} apiKey - Account API key
 * @property {string} groupId - Group ID (decimal or hex string)
 * @property {string} actionIpfsCid - IPFS CID for the action (keccak256-hashed on server)
 * @property {string} name - Action name
 * @property {string} description - Action description
 */

/**
 * @typedef {Object} UpdateUsageApiKeyMetadataOptions
 * @property {string} apiKey - Account API key
 * @property {string} usageApiKey - Usage API key to update
 * @property {string} name - Name
 * @property {string} description - Description
 */

/**
 * @typedef {Object} ListPageOptions
 * @property {string} apiKey - Account API key
 * @property {string} [pageNumber='0'] - Page number (0-based)
 * @property {string} [pageSize='10'] - Page size
 */

/**
 * @typedef {Object} ListPageWithGroupOptions
 * @property {string} apiKey - Account API key
 * @property {string} groupId - Group ID (decimal or hex string)
 * @property {string} [pageNumber='0'] - Page number (0-based)
 * @property {string} [pageSize='10'] - Page size
 */

/**
 * @typedef {Object} ListMetadataItem - One item from list_groups or list_actions
 * @property {string} id - ID (decimal string from contract)
 * @property {string} name - Name
 * @property {string} description - Description
 */

/**
 * @typedef {Object} WalletItem - One item from list_wallets or list_wallets_in_group (response.rs WalletItem)
 * @property {string} id - ID (hash as stored on chain)
 * @property {string} name - Name
 * @property {string} description - Description
 * @property {string} wallet_address - Wallet address (hex)
 */

/**
 * @typedef {Object} ApiKeyItem - One item from list_api_keys (response.rs ApiKeyItem)
 * @property {string} id - Auto-increment metadata ID (0x-prefixed hex)
 * @property {string} api_key_hash - keccak256 hash of the usage API key string (0x-prefixed hex, 66 chars)
 * @property {string} name - Name
 * @property {string} description - Description
 * @property {string} expiration - Expiration (unix timestamp string)
 * @property {number} balance - Balance (u64)
 * @property {boolean} can_create_groups - Permission to create groups
 * @property {boolean} can_delete_groups - Permission to delete groups
 * @property {boolean} can_create_pkps - Permission to create PKPs
 * @property {number[]} can_manage_ipfs_ids_in_groups - Group IDs allowed to manage IPFS action CIDs (0 = all groups)
 * @property {number[]} can_add_pkp_to_groups - Group IDs allowed to add PKPs (0 = all groups)
 * @property {number[]} can_remove_pkp_from_groups - Group IDs allowed to remove PKPs (0 = all groups)
 * @property {number[]} can_execute_in_groups - Group IDs allowed to execute actions (0 = all groups)
 */

/**
 * @typedef {Object} NodeChainConfigResponse - Node chain config (response.rs NodeChainConfigResponse)
 * @property {string} chain_name - Chain name
 * @property {number} chain_id - Chain ID
 * @property {boolean} is_evm - Whether the chain is EVM
 * @property {boolean} testnet - Whether the chain is a testnet
 * @property {string} token - Native token symbol
 * @property {string} [rpc_url] - RPC URL (resolved from chainlist when not in API response)
 * @property {string} contract_address - AccountConfig contract address
 */

// --- Response types (match core/v1/models/response.rs) ---

/**
 * @typedef {Object} NewAccountResponse
 * @property {string} api_key - Generated API key for the new account
 * @property {string} wallet_address - Wallet address for the new account
 */

/**
 * @typedef {Object} CreateWalletResponse
 * @property {string} wallet_address - Created wallet address
 */

/**
 * @typedef {Object} LitActionResponse - Lit action execution result (single response from /lit_action)
 * @property {*} response - Action response payload
 * @property {string} logs - Action logs
 * @property {boolean} has_error - Whether the action reported an error
 */

/**
 * @typedef {Object} AccountOpResponse - Response for account config operations (add_group, add_pkp_to_group, etc.)
 * @property {boolean} success
 */

/**
 * @typedef {Object} AddUsageApiKeyResponse - Response for add_usage_api_key (response.rs AddUsageApiKeyResponse)
 * @property {boolean} success
 * @property {string} usage_api_key - The newly created usage API key (returned only once)
 */

/**
 * Extract error message from API error response body. Handles { error }, { message }, or tuple-style [string].
 * @param {string} text - Raw response text
 * @returns {string|null} Extracted message or null
 */
function getErrorMessageFromBody(text) {
  if (!text || !text.trim()) return null;
  try {
    const body = JSON.parse(text);
    if (!body) return null;
    if (typeof body.error === 'string' && body.error.length > 0) return body.error;
    if (typeof body.message === 'string' && body.message.length > 0) return body.message;
    if (Array.isArray(body) && body.length === 1 && typeof body[0] === 'string') return body[0];
    if (typeof body === 'string') return body;
    return null;
  } catch (_) {
    return null;
  }
}

/**
 * Parse a fetch Response: on success return JSON body; on error throw with server message in body when present.
 * API error responses include a message in the body (e.g. { error: string } or { message: string }).
 * @param {Response} res - fetch Response
 * @param {string} context - Label for the request (e.g. "get_api_key")
 * @returns {Promise<any>} Parsed JSON body on success
 * @throws {Error} With server error message when res.ok is false (body message included when present)
 */
/**
 * Build headers with API key for endpoints that require authentication.
 * Sends X-Api-Key header (server also accepts Authorization: Bearer <key>).
 * @param {string} apiKey - Account or usage API key
 * @param {Object} [extra={}] - Additional headers (e.g. { 'Content-Type': 'application/json' })
 * @returns {Object} Headers object for fetch
 */
function headersWithApiKey(apiKey, extra = {}) {
  return {
    'X-Api-Key': apiKey,
    ...extra,
  };
}

async function parseResponse(res, context) {
  const text = await res.text();
  if (!res.ok) {
    const bodyMessage = getErrorMessageFromBody(text);
    const message = bodyMessage
      ? `${context}: ${bodyMessage}`
      : `${context}: ${res.status} ${res.statusText}`;
    throw new Error(message);
  }
  try {
    return text ? JSON.parse(text) : null;
  } catch (_) {
    throw new Error(`${context}: invalid JSON response`);
  }
}

const CHAINLIST_API = 'https://chainlistapi.com';
const CORS_PROXY = 'https://whateverorigin.org/get?url=';

/**
 * Resolve a public RPC URL for the given chain ID from chainlistapi.com.
 * Uses Whatever Origin CORS proxy (free, no domain whitelist) to avoid cross-origin restrictions.
 * @param {number} chainId - EVM chain ID
 * @returns {Promise<string|null>} First HTTP RPC URL, or null if not found
 */
export async function resolveRpcUrlFromChainlist(chainId) {
  if (chainId == null || chainId === '') return null;
  try {
    const url = `${CORS_PROXY}${encodeURIComponent(`${CHAINLIST_API}/chains/${chainId}`)}`;
    const res = await fetch(url);
    if (!res.ok) return null;
    const wrapper = await res.json();
    const data = typeof wrapper?.contents === 'string' ? JSON.parse(wrapper.contents) : wrapper;
    const rpcs = data?.rpc;
    if (!Array.isArray(rpcs)) return null;
    const entry = rpcs.find((r) => {
      const u = typeof r === 'string' ? r : r?.url;
      return typeof u === 'string' && u.startsWith('https://');
    });
    return entry ? (typeof entry === 'string' ? entry : entry.url) : null;
  } catch (_) {
    return null;
  }
}

export class LitNodeSimpleApiClient {
  /**
   * @param {Object} options
   * @param {string} [options.baseUrl='http://localhost:8000'] - Base URL of the API
   */
  constructor({ baseUrl = 'http://localhost:8000' } = {}) {
    const base = baseUrl.replace(/\/$/, '');
    this.baseUrl = `${base}/core/v1`;
  }

  /**
   * POST /core/v1/new_account
   * Creates a new account; server generates API key and wallet. Returns api_key and wallet_address.
   * @param {NewAccountOptions} options
   * @returns {Promise<NewAccountResponse>}
   */
  async newAccount({ accountName, accountDescription, email }) {
    const body = {
      account_name: accountName,
      account_description: accountDescription ?? '',
    };
    if (email) body.email = email;
    const res = await fetch(`${this.baseUrl}/new_account`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'new_account');
  }

  /**
   * GET /core/v1/account_exists
   * Checks whether an account exists and is mutable for the given API key (contract: accountExistsAndIsMutable).
   * API key via X-Api-Key or Authorization: Bearer header.
   * @param {string} apiKey - Account API key (base64-encoded)
   * @returns {Promise<boolean>}
   */
  async accountExists(apiKey) {
    const res = await fetch(`${this.baseUrl}/account_exists`, {
      headers: headersWithApiKey(apiKey),
    });
    return parseResponse(res, 'account_exists');
  }

  /**
   * GET /core/v1/create_wallet
   * Creates a wallet for the given API key and returns the wallet address.
   * API key via X-Api-Key or Authorization: Bearer header.
   * @param {string} apiKey - API key (from getApiKey)
   * @returns {Promise<CreateWalletResponse>}
   */
  async createWallet(apiKey) {
    const res = await fetch(`${this.baseUrl}/create_wallet`, {
      headers: headersWithApiKey(apiKey),
    });
    return parseResponse(res, 'create_wallet');
  }

  /**
   * POST /core/v1/get_lit_action_ipfs_id
   * Returns the IPFS CID (hash) for the given lit action code.
   * @param {string} code - Lit action JavaScript code
   * @returns {Promise<string>} IPFS CID (e.g. derived hash of code)
   */
  async getLitActionIpfsId(code) {
    const res = await fetch(`${this.baseUrl}/get_lit_action_ipfs_id`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(code),
    });
    return parseResponse(res, 'get_lit_action_ipfs_id');
  }

  /**
   * POST /core/v1/lit_action
   * Executes a lit action with the given code and optional params.
   * @param {LitActionOptions} options
   * @returns {Promise<LitActionResponse>} { response, logs, has_error }
   */
  async litAction({ apiKey, code, jsParams }) {
    const body = {
      code,
      js_params: jsParams ?? null,
    };
    const res = await fetch(`${this.baseUrl}/lit_action`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'lit_action');
  }

  /**
   * POST /core/v1/add_group
   * Add a group to an account with permitted action hashes and PKP hashes.
   * @param {AddGroupOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async addGroup({ apiKey, groupName, groupDescription = '', pkpIdsPermitted = [], cidHashesPermitted = [] }) {
    const body = {
      group_name: groupName ?? '',
      group_description: groupDescription ?? '',
      pkp_ids_permitted: pkpIdsPermitted,
      cid_hashes_permitted: cidHashesPermitted,
    };
    const res = await fetch(`${this.baseUrl}/add_group`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'add_group');
  }

  /**
   * POST /core/v1/add_action
   * Create a new action entry with name and description in the account's actionMetadata.
   * @param {AddActionOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async addAction({ apiKey, name, description }) {
    const body = { name: name ?? '', description: description ?? '' };
    const res = await fetch(`${this.baseUrl}/add_action`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'add_action');
  }

  /**
   * POST /core/v1/add_action_to_group
   * Add an action (IPFS CID) to a group. Use addAction separately to set name/description metadata.
   * @param {AddActionToGroupOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async addActionToGroup({ apiKey, groupId, actionIpfsCid }) {
    const body = {
      group_id: Number(groupId),
      action_ipfs_cid: actionIpfsCid,
    };
    const res = await fetch(`${this.baseUrl}/add_action_to_group`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'add_action_to_group');
  }

  /**
   * POST /core/v1/add_pkp_to_group
   * Add a PKP to a group.
   * @param {AddPkpToGroupOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async addPkpToGroup({ apiKey, groupId, pkpId }) {
    const body = {
      group_id: Number(groupId),
      pkp_id: pkpId,
    };
    const res = await fetch(`${this.baseUrl}/add_pkp_to_group`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'add_pkp_to_group');
  }

  /**
   * POST /core/v1/remove_pkp_from_group
   * Remove a PKP from a group.
   * @param {RemovePkpFromGroupOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async removePkpFromGroup({ apiKey, groupId, pkpId }) {
    const body = {
      group_id: Number(groupId),
      pkp_id: pkpId,
    };
    const res = await fetch(`${this.baseUrl}/remove_pkp_from_group`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'remove_pkp_from_group');
  }

  /**
   * POST /core/v1/add_usage_api_key
   * Add a usage API key to an account. Server creates and returns the new key.
   * @param {AddUsageApiKeyOptions} options
   * @returns {Promise<AddUsageApiKeyResponse>}
   */
  async addUsageApiKey({
    apiKey,
    name,
    description,
    canCreateGroups = false,
    canDeleteGroups = false,
    canCreatePkps = false,
    manageIpfsIdsInGroups = [],
    addPkpToGroups = [],
    removePkpFromGroups = [],
    executeInGroups = [],
  }) {
    const body = {
      name,
      description,
      can_create_groups: canCreateGroups,
      can_delete_groups: canDeleteGroups,
      can_create_pkps: canCreatePkps,
      manage_ipfs_ids_in_groups: manageIpfsIdsInGroups,
      add_pkp_to_groups: addPkpToGroups,
      remove_pkp_from_groups: removePkpFromGroups,
      execute_in_groups: executeInGroups,
    };
    const res = await fetch(`${this.baseUrl}/add_usage_api_key`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'add_usage_api_key');
  }

  /**
   * POST /core/v1/update_usage_api_key
   * Update metadata and permissions on an existing usage API key.
   * @param {UpdateUsageApiKeyOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async updateUsageApiKey({
    apiKey,
    usageApiKey,
    name,
    description,
    canCreateGroups = false,
    canDeleteGroups = false,
    canCreatePkps = false,
    manageIpfsIdsInGroups = [],
    addPkpToGroups = [],
    removePkpFromGroups = [],
    executeInGroups = [],
  }) {
    const body = {
      usage_api_key: usageApiKey,
      name,
      description,
      can_create_groups: canCreateGroups,
      can_delete_groups: canDeleteGroups,
      can_create_pkps: canCreatePkps,
      manage_ipfs_ids_in_groups: manageIpfsIdsInGroups,
      add_pkp_to_groups: addPkpToGroups,
      remove_pkp_from_groups: removePkpFromGroups,
      execute_in_groups: executeInGroups,
    };
    const res = await fetch(`${this.baseUrl}/update_usage_api_key`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'update_usage_api_key');
  }

  /**
   * POST /core/v1/remove_usage_api_key
   * Remove a usage API key from an account.
   * @param {RemoveUsageApiKeyOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async removeUsageApiKey({ apiKey, usageApiKey }) {
    const body = {
      usage_api_key: usageApiKey,
    };
    const res = await fetch(`${this.baseUrl}/remove_usage_api_key`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'remove_usage_api_key');
  }

  async removeGroup({ apiKey, groupId }) {
    const body = { group_id: String(groupId) };
    const res = await fetch(`${this.baseUrl}/remove_group`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'remove_group');
  }

  /**
   * POST /core/v1/update_group
   * Update group metadata and permission flags (AccountConfig.updateGroup).
   * @param {UpdateGroupOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async updateGroup({ apiKey, groupId, name, description, pkpIdsPermitted = [], cidHashesPermitted = [] }) {
    const body = {
      group_id: Number(groupId),
      name: name ?? '',
      description: description ?? '',
      pkp_ids_permitted: pkpIdsPermitted,
      cid_hashes_permitted: cidHashesPermitted,
    };
    const res = await fetch(`${this.baseUrl}/update_group`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'update_group');
  }

  /**
   * POST /core/v1/remove_action_from_group
   * Remove an action from a group by IPFS CID (keccak256-hashed on server).
   * @param {RemoveActionFromGroupOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async removeActionFromGroup({ apiKey, groupId, actionIpfsCid }) {
    const body = {
      group_id: Number(groupId),
      action_ipfs_cid: actionIpfsCid,
    };
    const res = await fetch(`${this.baseUrl}/remove_action_from_group`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'remove_action_from_group');
  }

  /**
   * POST /core/v1/update_action_metadata
   * Update action metadata (name, description) for an action in a group.
   * @param {UpdateActionMetadataOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async updateActionMetadata({ apiKey, groupId, actionIpfsCid, name, description }) {
    const body = {
      group_id: Number(groupId),
      action_ipfs_cid: actionIpfsCid,
      name: name ?? '',
      description: description ?? '',
    };
    const res = await fetch(`${this.baseUrl}/update_action_metadata`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'update_action_metadata');
  }

  /**
   * POST /core/v1/update_usage_api_key_metadata
   * Update usage API key metadata (name, description).
   * @param {UpdateUsageApiKeyMetadataOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async updateUsageApiKeyMetadata({ apiKey, usageApiKey, name, description }) {
    const body = {
      usage_api_key: usageApiKey,
      name: name ?? '',
      description: description ?? '',
    };
    const res = await fetch(`${this.baseUrl}/update_usage_api_key_metadata`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'update_usage_api_key_metadata');
  }

  /**
   * GET /core/v1/list_api_keys
   * List usage API keys for an account (paginated). Returns ApiKeyItem per key.
   * @param {ListPageOptions} options
   * @returns {Promise<ApiKeyItem[]>}
   */
  async listApiKeys({ apiKey, pageNumber = '0', pageSize = '10' }) {
    const params = new URLSearchParams({
      page_number: String(pageNumber),
      page_size: String(pageSize),
    });
    const res = await fetch(`${this.baseUrl}/list_api_keys?${params}`, {
      headers: headersWithApiKey(apiKey),
    });
    return parseResponse(res, 'list_api_keys');
  }

  /**
   * GET /core/v1/list_groups
   * List groups for an account (paginated). Returns metadata (id, name, description) per group.
   * @param {ListPageOptions} options
   * @returns {Promise<ListMetadataItem[]>}
   */
  async listGroups({ apiKey, pageNumber = '0', pageSize = '10' }) {
    const params = new URLSearchParams({
      page_number: String(pageNumber),
      page_size: String(pageSize),
    });
    const res = await fetch(`${this.baseUrl}/list_groups?${params}`, {
      headers: headersWithApiKey(apiKey),
    });
    return parseResponse(res, 'list_groups');
  }

  /**
   * GET /core/v1/list_wallets
   * List wallets (wallet derivation metadata) for an account (paginated).
   * @param {ListPageOptions} options
   * @returns {Promise<WalletItem[]>}
   */
  async listWallets({ apiKey, pageNumber = '0', pageSize = '10' }) {
    const params = new URLSearchParams({
      page_number: String(pageNumber),
      page_size: String(pageSize),
    });
    const res = await fetch(`${this.baseUrl}/list_wallets?${params}`, {
      headers: headersWithApiKey(apiKey),
    });
    return parseResponse(res, 'list_wallets');
  }

  /**
   * GET /core/v1/list_wallets_in_group
   * List wallets in a group (paginated). Returns WalletItem for each wallet in the group.
   * @param {ListPageWithGroupOptions} options
   * @returns {Promise<WalletItem[]>}
   */
  async listWalletsInGroup({ apiKey, groupId, pageNumber = '0', pageSize = '10' }) {
    const params = new URLSearchParams({
      group_id: groupId,
      page_number: String(pageNumber),
      page_size: String(pageSize),
    });
    const res = await fetch(`${this.baseUrl}/list_wallets_in_group?${params}`, {
      headers: headersWithApiKey(apiKey),
    });
    return parseResponse(res, 'list_wallets_in_group');
  }

  /**
   * GET /core/v1/list_actions
   * List actions in a group (paginated). Returns metadata (id, name, description) per action.
   * @param {ListPageWithGroupOptions} options
   * @returns {Promise<ListMetadataItem[]>}
   */
  async listActions({ apiKey, groupId, pageNumber = '0', pageSize = '10' }) {
    const params = new URLSearchParams({
      group_id: groupId,
      page_number: String(pageNumber),
      page_size: String(pageSize),
    });
    const res = await fetch(`${this.baseUrl}/list_actions?${params}`, {
      headers: headersWithApiKey(apiKey),
    });
    return parseResponse(res, 'list_actions');
  }

  /**
   * GET /core/v1/get_node_chain_config
   * Returns the node's chain configuration (chain name, id, RPC URL, contract address, etc.).
   * When the API does not include rpc_url, it is resolved from chainlistapi.com using chain_id.
   * @returns {Promise<NodeChainConfigResponse>}
   */
  async getNodeChainConfig() {
    const res = await fetch(`${this.baseUrl}/get_node_chain_config`);
    const cfg = await parseResponse(res, 'get_node_chain_config');
    if (!cfg.rpc_url && cfg.chain_id != null && cfg.is_evm) {
      const rpcUrl = await resolveRpcUrlFromChainlist(cfg.chain_id);
      if (rpcUrl) cfg.rpc_url = rpcUrl;
    }
    return cfg;
  }

  /**
   * GET /core/v1/get_api_payers
   * Returns all API payer addresses registered on the node.
   * @returns {Promise<string[]>}
   */
  async getApiPayers() {
    const res = await fetch(`${this.baseUrl}/get_api_payers`);
    return parseResponse(res, 'get_api_payers');
  }

  /**
   * GET /core/v1/get_admin_api_payer
   * Returns the admin API payer address for the node.
   * @returns {Promise<string>}
   */
  async getAdminApiPayer() {
    const res = await fetch(`${this.baseUrl}/get_admin_api_payer`);
    return parseResponse(res, 'get_admin_api_payer');
  }

  // ─── Billing ─────────────────────────────────────────────────────────────

  /**
   * GET /core/v1/billing/stripe_config
   * Returns the Stripe publishable key for Stripe.js initialisation.
   * @returns {Promise<{publishable_key: string}>}
   */
  async getStripeConfig() {
    const res = await fetch(`${this.baseUrl}/billing/stripe_config`);
    return parseResponse(res, 'billing/stripe_config');
  }

  /**
   * GET /core/v1/billing/balance
   * Returns the current credit balance for the authenticated API key.
   * @param {string} apiKey
   * @returns {Promise<{balance_cents: number, balance_display: string}>}
   */
  async getBillingBalance(apiKey) {
    const res = await fetch(`${this.baseUrl}/billing/balance`, {
      headers: headersWithApiKey(apiKey),
    });
    return parseResponse(res, 'billing/balance');
  }

  /**
   * POST /core/v1/billing/create_payment_intent
   * Creates a Stripe PaymentIntent; returns client_secret and payment_intent_id.
   * @param {string} apiKey
   * @param {number} amountCents - Amount in US cents (minimum 500)
   * @returns {Promise<{client_secret: string, payment_intent_id: string}>}
   */
  async createPaymentIntent(apiKey, amountCents) {
    const res = await fetch(`${this.baseUrl}/billing/create_payment_intent`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify({ amount_cents: amountCents }),
    });
    return parseResponse(res, 'billing/create_payment_intent');
  }

  /**
   * POST /core/v1/billing/confirm_payment
   * Verifies a succeeded PaymentIntent and credits the account.
   * @param {string} apiKey
   * @param {string} paymentIntentId
   * @returns {Promise<void>}
   */
  async confirmPayment(apiKey, paymentIntentId) {
    const res = await fetch(`${this.baseUrl}/billing/confirm_payment`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify({ payment_intent_id: paymentIntentId }),
    });
    return parseResponse(res, 'billing/confirm_payment');
  }
}

/**
 * Factory for a default client (e.g. for script usage).
 * @param {string} [baseUrl='http://localhost:8000']
 * @returns {LitNodeSimpleApiClient}
 */
export function createClient(baseUrl = 'http://localhost:8000') {
  return new LitNodeSimpleApiClient({ baseUrl });
}

export default LitNodeSimpleApiClient;
