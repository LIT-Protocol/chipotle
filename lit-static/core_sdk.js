/**
 * Lit Node Simple API - JavaScript Core SDK
 *
 * Wrapper for the v1 API endpoints defined in lit-api-server.
 * Types match core/v1/models (request.rs, response.rs) and core/v1/endpoints.rs.
 * Routes are mounted at /core/v1/ (see src/main.rs).
 *
 * Modes:
 *  - 'api'       (default): every call goes through lit-api-server over HTTP.
 *  - 'sovereign'          : reads call the AccountConfig contract directly via
 *                           RPC (no server round-trip). Writes are submitted
 *                           as wallet-signed contract transactions once
 *                           `connectSigner(signer)` has been called; without a
 *                           signer, write methods throw.
 *
 * In sovereign mode the constructor requires `rpcUrl` + `contractAddress`, or
 * the dashboard can call `getNodeChainConfig()` first and pass the values in.
 */

import { ACCOUNT_CONFIG_VIEW_ABI } from './account_config_view_abi.js';
import {
  ACCOUNT_CONFIG_FULL_ABI,
  ACCOUNT_CONFIG_ABI_VERSION,
  mergeDeployments,
  isAbiDriftDevOverrideEnabled,
} from './account_config_full_abi.js';
import { runContractWrite, TX_STATES } from './tx_lifecycle.js';

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
 * @property {string} actionIpfsCid - IPFS CID for the action (keccak256-hashed on server)
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
 * @typedef {Object} DeleteActionOptions
 * @property {string} apiKey - Account API key
 * @property {string} hashedCid - Already-hashed CID for the action (0x-prefixed keccak256 hex string)
 */

/**
 * @typedef {Object} RemoveActionFromGroupOptions
 * @property {string} apiKey - Account API key
 * @property {string} groupId - Group ID (decimal or hex string)
 * @property {string} hashedCid - Already-hashed CID for the action (0x-prefixed keccak256 hex string)
 */

/**
 * @typedef {Object} UpdateActionMetadataOptions
 * @property {string} apiKey - Account API key
 * @property {string} hashedCid - Already-hashed CID for the action (0x-prefixed keccak256 hex string)
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

const ETHERS_CDN_URL = 'https://cdn.jsdelivr.net/npm/ethers@6.13.0/dist/ethers.min.js';

let _ethersPromise = null;
/**
 * Lazily load ethers v6 from CDN (once per page). Returns the ethers module.
 * Works in the dashboard and monitor without adding a build step. In environments
 * where ethers is already a global (e.g. when bundled alongside the SDK), it
 * resolves to that instance.
 */
async function loadEthers() {
  if (typeof globalThis !== 'undefined' && globalThis.ethers) return globalThis.ethers;
  if (!_ethersPromise) {
    _ethersPromise = import(/* @vite-ignore */ ETHERS_CDN_URL).then((mod) => {
      const e = mod.ethers ?? mod.default ?? mod;
      if (typeof globalThis !== 'undefined') globalThis.ethers = e;
      return e;
    });
  }
  return _ethersPromise;
}

export class LitNodeSimpleApiClient {
  /**
   * @param {Object} options
   * @param {string} [options.baseUrl='http://localhost:8000'] - Base URL of the API
   * @param {'api'|'sovereign'} [options.mode='api'] - Read-path mode. See module docblock.
   * @param {string} [options.rpcUrl] - RPC URL for sovereign reads. Required when mode === 'sovereign'.
   * @param {string} [options.contractAddress] - AccountConfig contract address. Required when mode === 'sovereign'.
   * @param {number} [options.chainId] - Expected chain ID. Used for drift pinning + wallet chain guard. Optional; auto-detected from RPC when omitted.
   * @param {Object} [options.deployments] - Optional override map `{ "<chainId>:<address>": { runtimeBytecodeKeccak } }` for drift pinning.
   * @param {import('ethers').Signer} [options.signer] - Pre-connected signer for sovereign writes. May be set later via connectSigner().
   */
  constructor({ baseUrl = 'http://localhost:8000', mode = 'api', rpcUrl, contractAddress, chainId, deployments, signer, adminHashOverride } = {}) {
    if (typeof baseUrl !== 'string') {
      throw new Error(
        `LitNodeSimpleApiClient: baseUrl must be a string, got ${typeof baseUrl} (${JSON.stringify(baseUrl)})`,
      );
    }
    const base = baseUrl.replace(/\/$/, '');
    this.baseUrl = `${base}/core/v1`;
    this.mode = mode;
    this.rpcUrl = rpcUrl ?? null;
    this.contractAddress = contractAddress ?? null;
    this.chainId = chainId ?? null;
    this.deployments = mergeDeployments(deployments);
    this.signer = signer ?? null;
    // When set (e.g. ChainSecured login: keccak256(abi.encodePacked(address))),
    // `_adminHash(apiKey)` returns this and ignores the apiKey argument.
    // Leave null for API-mode and legacy sovereign API-key sessions, which
    // still derive the identity hash from the api key string itself.
    this.adminHashOverride = adminHashOverride ?? null;
    this._viewContractPromise = null;
    this._writeContract = null;
    this._driftCheckPromise = null;

    if (mode === 'sovereign' && (!rpcUrl || !contractAddress)) {
      throw new Error(
        'LitNodeSimpleApiClient: sovereign mode requires rpcUrl and contractAddress',
      );
    }
  }

  /**
   * Attach a signer to this client for sovereign-mode writes. Must be called
   * (or passed via constructor) before any write method in sovereign mode.
   * @param {import('ethers').Signer} signer - ethers v6 Signer
   */
  connectSigner(signer) {
    this.signer = signer;
    this._writeContract = null;
  }

  /**
   * Throws unless sovereign mode is set up with a usable signer.
   */
  _assertSovereignWriteReady() {
    if (this.mode !== 'sovereign') {
      throw new Error('_assertSovereignWriteReady called outside sovereign mode');
    }
    if (!this.signer) {
      throw new Error(
        'LitNodeSimpleApiClient: sovereign write requires a connected signer. Call connectSigner(signer) first.',
      );
    }
  }

  /**
   * Verify on-chain bytecode matches the pinned hash for (chainId, address).
   * Hard-blocks sovereign writes on mismatch. Runs once per client instance;
   * cached promise is reused for subsequent calls.
   *
   * Fail-closed: if no entry exists for (chainId, address), hard-block sovereign
   * writes unless LIT_ACCOUNT_CONFIG_ALLOW_UNPINNED_DEPLOYMENTS is set (dev-only).
   * Operators MUST populate ACCOUNT_CONFIG_DEPLOYMENTS or pass `deployments`.
   */
  async _verifyAbiIntegrity() {
    if (this.mode !== 'sovereign') return;
    if (!this._driftCheckPromise) {
      this._driftCheckPromise = (async () => {
        const ethers = await loadEthers();
        const provider = new ethers.JsonRpcProvider(this.rpcUrl);
        const [code, network] = await Promise.all([
          provider.getCode(this.contractAddress),
          provider.getNetwork(),
        ]);
        const chainId = Number(network.chainId);
        if (this.chainId == null) this.chainId = chainId;
        if (!code || code === '0x') {
          throw new Error(
            `ABI drift check: no contract code at ${this.contractAddress} on chain ${chainId}. Aborting sovereign writes.`,
          );
        }
        const liveHash = ethers.keccak256(code).toLowerCase();
        const key = `${chainId}:${this.contractAddress.toLowerCase()}`;
        const pinned = this.deployments[key];
        if (!pinned) {
          if (!isAbiDriftDevOverrideEnabled()) {
            throw new Error(
              `ABI drift check: no pinned entry for ${key}. Add one to ACCOUNT_CONFIG_DEPLOYMENTS or pass 'deployments' option. To run against an unpinned deployment (dev only), set window.LIT_ACCOUNT_CONFIG_ALLOW_UNPINNED_DEPLOYMENTS = true. (ABI version: ${ACCOUNT_CONFIG_ABI_VERSION})`,
            );
          }
          console.warn(
            `[LitNodeSimpleApiClient] ABI drift check SKIPPED (dev override): no pinned entry for ${key}. (ABI version: ${ACCOUNT_CONFIG_ABI_VERSION})`,
          );
          return;
        }
        if (pinned.runtimeBytecodeKeccak === null) {
          // Explicit dev-only opt-out.
          return;
        }
        if (pinned.runtimeBytecodeKeccak.toLowerCase() !== liveHash) {
          throw new Error(
            `ABI drift detected for ${key}: live bytecode hash ${liveHash} != pinned ${pinned.runtimeBytecodeKeccak.toLowerCase()}. Dashboard must be redeployed with a matching ABI bundle. (ABI version: ${ACCOUNT_CONFIG_ABI_VERSION})`,
          );
        }
      })().catch((err) => {
        // Surface the error on every retry (don't cache the failure forever).
        this._driftCheckPromise = null;
        throw err;
      });
    }
    return this._driftCheckPromise;
  }

  /**
   * Lazily build (and cache) a writeable ethers.Contract bound to the full
   * ABI + connected signer. Runs the ABI drift check on first construction.
   * @returns {Promise<Object>} ethers.Contract
   */
  async _getWriteContract() {
    this._assertSovereignWriteReady();
    if (!this._writeContract) {
      await this._verifyAbiIntegrity();
      const ethers = await loadEthers();
      this._writeContract = new ethers.Contract(this.contractAddress, ACCOUNT_CONFIG_FULL_ABI, this.signer);
    }
    return this._writeContract;
  }

  /**
   * Lazily build (and cache) a read-only ethers.Contract instance bound to the
   * AccountConfig address. Uses the view-only ABI subset.
   * @returns {Promise<Object>} ethers.Contract
   */
  async _getViewContract() {
    if (this.mode !== 'sovereign') {
      throw new Error('_getViewContract called outside sovereign mode');
    }
    if (!this._viewContractPromise) {
      this._viewContractPromise = (async () => {
        const ethers = await loadEthers();
        const provider = new ethers.JsonRpcProvider(this.rpcUrl);
        return new ethers.Contract(this.contractAddress, ACCOUNT_CONFIG_VIEW_ABI, provider);
      })();
    }
    return this._viewContractPromise;
  }

  /**
   * Keccak256 hash of an API key string, as uint256 — matches server-side
   * `api_key_hash` in lit-api-server/src/utils/parse_with_hash.rs.
   * @param {string} apiKey
   * @returns {Promise<string>} 0x-prefixed 32-byte hex, passable as uint256.
   */
  async _apiKeyHash(apiKey) {
    const ethers = await loadEthers();
    return ethers.keccak256(ethers.toUtf8Bytes(apiKey));
  }

  /**
   * Resolve the 32-byte `apiKeyHash` used as the account identity on-chain.
   * ChainSecured sessions set `adminHashOverride = keccak256(abi.encodePacked(address))`
   * at login; API-key sessions (API mode or legacy sovereign) leave it null
   * and this falls through to `_apiKeyHash(apiKey)`.
   *
   * IMPORTANT: only call this at identity sites (the "whose account is it"
   * hash). Content hashes for actionIpfsCid and pkp addresses must keep
   * calling `_apiKeyHash(value)` directly so the override does not corrupt
   * them.
   *
   * @param {string} apiKey - apiKey string; may be empty for ChainSecured sessions.
   * @returns {Promise<string>} 0x-prefixed 32-byte hex.
   */
  async _adminHash(apiKey) {
    if (this.adminHashOverride) return this.adminHashOverride;
    return this._apiKeyHash(apiKey);
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
   * Contract: WritesFacet.newChainSecuredAccount(string, string).
   * Creates a ChainSecured (unmanaged, wallet-signed) account. The connected
   * signer address IS the admin; on-chain, apiKeyHash = keccak256(abi.encodePacked(msg.sender)).
   *
   * Sovereign mode only. Requires a connected signer + the chain drift check
   * to pass. Reverts on collision (existing account for this wallet).
   *
   * @param {Object} options
   * @param {string} options.accountName
   * @param {string} options.accountDescription
   * @param {Object} [options.sovereignLifecycle] - injected by the dashboard Proxy
   * @returns {Promise<{wallet_address: string, api_key_hash: string, transaction_hash: string}>}
   */
  async newChainSecuredAccount({ accountName, accountDescription, sovereignLifecycle } = {}) {
    if (this.mode !== 'sovereign') {
      throw new Error('newChainSecuredAccount requires sovereign mode');
    }
    const contract = await this._getWriteContract();
    const ethers = await loadEthers();
    const signerAddress = await this.signer.getAddress();
    const apiKeyHash = ethers.solidityPackedKeccak256(['address'], [signerAddress]);
    try {
      const { txHash } = await runContractWrite({
        contract, method: 'newChainSecuredAccount',
        args: [accountName ?? '', accountDescription ?? ''],
        ...(sovereignLifecycle ?? {}),
      });
      return { wallet_address: signerAddress, api_key_hash: apiKeyHash, transaction_hash: txHash };
    } catch (err) {
      const msg = err?.decoded || err?.message || '';
      if (msg.includes('AccountAlreadyExists')) {
        const friendly = new Error(
          'An account already exists for this wallet address. Connect a different wallet or log in with your existing account.',
        );
        friendly.cause = err;
        friendly.accountAlreadyExists = true;
        throw friendly;
      }
      throw err;
    }
  }

  /**
   * GET /core/v1/account_exists
   * Checks whether an account exists and is mutable for the given API key (contract: accountExistsAndIsMutable).
   * API key via X-Api-Key or Authorization: Bearer header.
   * @param {string} apiKey - Account API key (base64-encoded)
   * @returns {Promise<boolean>}
   */
  async accountExists(apiKey) {
    if (this.mode === 'sovereign') {
      const contract = await this._getViewContract();
      const hash = await this._adminHash(apiKey);
      // accountExistsAndIsMutable has an msg.sender check (caller must be an
      // api_payer). The server-side path spoofs this by setting `.from(api_payer)`
      // on the eth_call. For sovereign reads we take the same approach: fetch
      // api_payers via the same contract and use the first one as the from address.
      const payers = await contract.api_payers();
      if (!payers || payers.length === 0) {
        throw new Error('account_exists (sovereign): no api_payers configured on contract');
      }
      return await contract.accountExistsAndIsMutable.staticCall(hash, { from: payers[0] });
    }
    const res = await fetch(`${this.baseUrl}/account_exists`, {
      headers: headersWithApiKey(apiKey),
    });
    return parseResponse(res, 'account_exists');
  }

  /**
   * Sovereign-only variant of `accountExists` that takes a pre-computed
   * apiKeyHash (0x-prefixed uint256 hex). Used by the ChainSecured login
   * flow where the dashboard computes `keccak256(abi.encodePacked(address))`
   * before it has (or needs) an apiKey string.
   *
   * Uses the same api_payer spoofing pattern as `accountExists` sovereign.
   *
   * @param {string} apiKeyHashHex - 0x-prefixed 32-byte hex
   * @returns {Promise<boolean>}
   */
  async accountExistsByHash(apiKeyHashHex) {
    if (this.mode !== 'sovereign') {
      throw new Error('accountExistsByHash requires sovereign mode');
    }
    const contract = await this._getViewContract();
    const payers = await contract.api_payers();
    if (!payers || payers.length === 0) {
      throw new Error('accountExistsByHash: no api_payers configured on contract');
    }
    return await contract.accountExistsAndIsMutable.staticCall(apiKeyHashHex, { from: payers[0] });
  }

  /**
   * Public wrapper around `_verifyAbiIntegrity` that uses a read-only provider
   * (no wallet required) and returns a non-throwing result. Safe to call
   * post-wallet-connect to drive the drift banner.
   *
   * @returns {Promise<{ok: true} | {ok: false, reason: string}>}
   */
  async checkAbiDrift() {
    if (this.mode !== 'sovereign') {
      return { ok: true };
    }
    try {
      await this._verifyAbiIntegrity();
      return { ok: true };
    } catch (err) {
      return { ok: false, reason: err?.message || String(err) };
    }
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
   * @returns {Promise<AddGroupResponse>} Response with `success` and `group_id`.
   */
  async addGroup({ apiKey, groupName, groupDescription = '', pkpIdsPermitted = [], cidHashesPermitted = [], sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const name = groupName ?? '';
      const description = groupDescription ?? '';
      // staticCall first to pre-fetch the new group_id without burning a tx.
      let groupId = null;
      try {
        groupId = await contract.addGroup.staticCall(hash, name, description, cidHashesPermitted, pkpIdsPermitted);
      } catch (e) {
        // staticCall revert will surface again on the real call; proceed and
        // let runContractWrite produce a decoded error.
      }
      const { txHash } = await runContractWrite({
        contract, method: 'addGroup',
        args: [hash, name, description, cidHashesPermitted, pkpIdsPermitted],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, group_id: groupId != null ? groupId.toString() : null, transaction_hash: txHash };
    }
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
  async addAction({ apiKey, actionIpfsCid, name, description, sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const actionHash = await this._apiKeyHash(actionIpfsCid);
      const { txHash } = await runContractWrite({
        contract, method: 'addAction',
        args: [hash, name ?? '', description ?? '', actionHash],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, action_hash: actionHash, transaction_hash: txHash };
    }
    const body = { action_ipfs_cid: actionIpfsCid, name: name ?? '', description: description ?? '' };
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
  async addActionToGroup({ apiKey, groupId, actionIpfsCid, sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const actionHash = await this._apiKeyHash(actionIpfsCid);
      const { txHash } = await runContractWrite({
        contract, method: 'addActionToGroup',
        args: [hash, BigInt(groupId), actionHash],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
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
  async addPkpToGroup({ apiKey, groupId, pkpId, sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const { txHash } = await runContractWrite({
        contract, method: 'addPkpToGroup',
        args: [hash, BigInt(groupId), pkpId],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
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
  async removePkpFromGroup({ apiKey, groupId, pkpId, sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const { txHash } = await runContractWrite({
        contract, method: 'removePkpFromGroup',
        args: [hash, BigInt(groupId), pkpId],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
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
    expiration,
    balance,
    sovereignLifecycle,
  } = {}) {
    if (this.mode === 'sovereign') {
      const ethers = await loadEthers();
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      // Sovereign mode has no server-side Stripe billing: balance starts at 0
      // and can't be topped up without API mode. Caller may pass expiration
      // (uint256 seconds timestamp; 0 = never) and balance explicitly.
      const expirationVal = expiration != null ? BigInt(expiration) : 0n;
      const balanceVal = balance != null ? BigInt(balance) : 0n;
      // Client-generated secret: server-mode generates this server-side and
      // returns it. Sovereign must mint it locally so the user can save the
      // cleartext key before it leaves this browser.
      const randBytes = new Uint8Array(32);
      crypto.getRandomValues(randBytes);
      const newUsageKey = 'lk_' + Array.from(randBytes, (b) => b.toString(16).padStart(2, '0')).join('');
      const usageHash = ethers.keccak256(ethers.toUtf8Bytes(newUsageKey));
      const { txHash } = await runContractWrite({
        contract, method: 'setUsageApiKey',
        args: [
          hash,
          usageHash,
          expirationVal,
          balanceVal,
          name ?? '',
          description ?? '',
          canCreateGroups,
          canDeleteGroups,
          canCreatePkps,
          manageIpfsIdsInGroups.map((n) => BigInt(n)),
          addPkpToGroups.map((n) => BigInt(n)),
          removePkpFromGroups.map((n) => BigInt(n)),
          executeInGroups.map((n) => BigInt(n)),
        ],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, api_key: newUsageKey, hash: usageHash, transaction_hash: txHash };
    }
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
    expiration,
    balance,
    sovereignLifecycle,
  } = {}) {
    if (this.mode === 'sovereign') {
      const ethers = await loadEthers();
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const usageHash = ethers.keccak256(ethers.toUtf8Bytes(usageApiKey));
      // setUsageApiKey is an upsert; existing expiration/balance are
      // overwritten. Sovereign callers should fetch current values via
      // listApiKeys and pass them back if preservation is desired.
      const expirationVal = expiration != null ? BigInt(expiration) : 0n;
      const balanceVal = balance != null ? BigInt(balance) : 0n;
      const { txHash } = await runContractWrite({
        contract, method: 'setUsageApiKey',
        args: [
          hash,
          usageHash,
          expirationVal,
          balanceVal,
          name ?? '',
          description ?? '',
          canCreateGroups,
          canDeleteGroups,
          canCreatePkps,
          manageIpfsIdsInGroups.map((n) => BigInt(n)),
          addPkpToGroups.map((n) => BigInt(n)),
          removePkpFromGroups.map((n) => BigInt(n)),
          executeInGroups.map((n) => BigInt(n)),
        ],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
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
  async removeUsageApiKey({ apiKey, usageApiKey, sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const ethers = await loadEthers();
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const usageHash = ethers.keccak256(ethers.toUtf8Bytes(usageApiKey));
      const { txHash } = await runContractWrite({
        contract, method: 'removeUsageApiKey',
        args: [hash, usageHash],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
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

  async removeGroup({ apiKey, groupId, sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const { txHash } = await runContractWrite({
        contract, method: 'removeGroup',
        args: [hash, BigInt(groupId)],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
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
  async updateGroup({ apiKey, groupId, name, description, pkpIdsPermitted = [], cidHashesPermitted = [], sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const { txHash } = await runContractWrite({
        contract, method: 'updateGroup',
        args: [hash, BigInt(groupId), name ?? '', description ?? '', cidHashesPermitted, pkpIdsPermitted],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
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
   * POST /core/v1/delete_action
   * Delete an action and its metadata from the account.
   * @param {DeleteActionOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async deleteAction({ apiKey, hashedCid, sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const { txHash } = await runContractWrite({
        contract, method: 'removeAction',
        args: [hash, hashedCid],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
    const body = { hashed_cid: hashedCid };
    const res = await fetch(`${this.baseUrl}/delete_action`, {
      method: 'POST',
      headers: headersWithApiKey(apiKey, { 'Content-Type': 'application/json' }),
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'delete_action');
  }

  /**
   * POST /core/v1/remove_action_from_group
   * Remove an action from a group by already-hashed CID.
   * @param {RemoveActionFromGroupOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async removeActionFromGroup({ apiKey, groupId, hashedCid, sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const { txHash } = await runContractWrite({
        contract, method: 'removeActionFromGroup',
        args: [hash, BigInt(groupId), hashedCid],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
    const body = {
      group_id: Number(groupId),
      hashed_cid: hashedCid,
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
   * Update action metadata (name, description) for an action.
   * @param {UpdateActionMetadataOptions} options
   * @returns {Promise<AccountOpResponse>}
   */
  async updateActionMetadata({ apiKey, hashedCid, name, description, groupId = 0, sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      // groupId === 0 means "account-level action metadata" per contract convention.
      const { txHash } = await runContractWrite({
        contract, method: 'updateActionMetadata',
        args: [hash, hashedCid, BigInt(groupId), name ?? '', description ?? ''],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
    const body = {
      hashed_cid: hashedCid,
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
  async updateUsageApiKeyMetadata({ apiKey, usageApiKey, name, description, sovereignLifecycle } = {}) {
    if (this.mode === 'sovereign') {
      const ethers = await loadEthers();
      const contract = await this._getWriteContract();
      const hash = await this._adminHash(apiKey);
      const usageHash = ethers.keccak256(ethers.toUtf8Bytes(usageApiKey));
      const { txHash } = await runContractWrite({
        contract, method: 'updateUsageApiKeyMetadata',
        args: [hash, usageHash, name ?? '', description ?? ''],
        ...(sovereignLifecycle ?? {}),
      });
      return { success: true, transaction_hash: txHash };
    }
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
    if (this.mode === 'sovereign') {
      const contract = await this._getViewContract();
      const hash = await this._adminHash(apiKey);
      const rows = await contract.listApiKeys(hash, pageNumber, pageSize);
      return rows.map((r) => ({
        id: r.metadata.id.toString(),
        api_key_hash: '0x' + r.apiKeyHash.toString(16).padStart(64, '0'),
        name: r.metadata.name,
        description: r.metadata.description,
        expiration: r.expiration.toString(),
        balance: Number(r.balance),
        can_create_groups: r.createGroups,
        can_delete_groups: r.deleteGroups,
        can_create_pkps: r.createPKPs,
        can_manage_ipfs_ids_in_groups: r.manageIPFSIdsInGroups.map((n) => Number(n)),
        can_add_pkp_to_groups: r.addPkpToGroups.map((n) => Number(n)),
        can_remove_pkp_from_groups: r.removePkpFromGroups.map((n) => Number(n)),
        can_execute_in_groups: r.executeInGroups.map((n) => Number(n)),
      }));
    }
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
    if (this.mode === 'sovereign') {
      const contract = await this._getViewContract();
      const hash = await this._adminHash(apiKey);
      const rows = await contract.listGroups(hash, pageNumber, pageSize);
      return rows.map((r) => ({
        id: r.id.toString(),
        name: r.name,
        description: r.description,
      }));
    }
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
    if (this.mode === 'sovereign') {
      const contract = await this._getViewContract();
      const hash = await this._adminHash(apiKey);
      const rows = await contract.listPkps(hash, pageNumber, pageSize);
      return rows.map((r) => ({
        id: r.id.toString(),
        name: r.name,
        description: r.description,
        wallet_address: r.pkpId,
      }));
    }
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
    if (this.mode === 'sovereign') {
      const contract = await this._getViewContract();
      const hash = await this._adminHash(apiKey);
      const rows = await contract.listWalletsInGroup(hash, groupId, pageNumber, pageSize);
      return rows.map((r) => ({
        id: r.id.toString(),
        name: r.name,
        description: r.description,
        wallet_address: r.pkpId,
      }));
    }
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
   * List actions (paginated). When groupId is provided, lists actions in that group.
   * When omitted, lists all actions on the account.
   * @param {Object} options
   * @param {string} options.apiKey - Account API key
   * @param {string} [options.groupId] - Group ID. If omitted, lists all account-level actions.
   * @param {string} [options.pageNumber='0'] - Page number (0-based)
   * @param {string} [options.pageSize='10'] - Page size
   * @returns {Promise<ListMetadataItem[]>}
   */
  async listActions({ apiKey, groupId, pageNumber = '0', pageSize = '10' }) {
    if (this.mode === 'sovereign') {
      const contract = await this._getViewContract();
      const hash = await this._adminHash(apiKey);
      // Match server semantics: group IDs start at 1, so groupId==0 means
      // "account-level listing", not "group zero". listActionsInGroup with 0
      // can revert (GroupDoesNotExist) or return wrong data.
      const inGroup = groupId !== undefined && Number(groupId) > 0;
      const rows = inGroup
        ? await contract.listActionsInGroup(hash, groupId, pageNumber, pageSize)
        : await contract.listActions(hash, pageNumber, pageSize);
      return rows.map((r) => ({
        id: r.id.toString(),
        name: r.name,
        description: r.description,
      }));
    }
    const entries = {
      page_number: String(pageNumber),
      page_size: String(pageSize),
    };
    if (groupId !== undefined) {
      entries.group_id = groupId;
    }
    const params = new URLSearchParams(entries);
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
    if (this.mode === 'sovereign') {
      const contract = await this._getViewContract();
      return await contract.api_payers();
    }
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
 * Accepts either a string baseUrl or a full options object.
 * @param {string|Object} [options='http://localhost:8000']
 * @returns {LitNodeSimpleApiClient}
 */
export function createClient(options = 'http://localhost:8000') {
  const opts = typeof options === 'string' ? { baseUrl: options } : options;
  return new LitNodeSimpleApiClient(opts);
}

export default LitNodeSimpleApiClient;
