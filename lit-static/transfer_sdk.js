/**
 * Lit Node Simple API - JavaScript Transfer SDK
 *
 * Wrapper for transfer endpoints in abstractions/transfer/endpoints.rs.
 * Routes are mounted at /transfer/v1/ (see src/main.rs).
 */

/**
 * @typedef {Object} GetBalanceResponse
 * @property {string} address - Wallet address
 * @property {number} balance - Balance in ether (native token)
 * @property {string} chain - Chain key (e.g. "ethereum", "solana")
 * @property {string} symbol - Asset symbol
 */

/**
 * @typedef {Object} TransferOptions
 * @property {string} apiKey - Hex-encoded API key (from getApiKey)
 * @property {string} pkpPublicKey - PKP public key
 * @property {string} chain - Chain key (lowercase, e.g. "ethereum", "solana"); use ChainInfoItem.chain from get_chains
 * @property {string} destinationAddress - Destination address
 * @property {number} amount - Amount in ether (native token)
 */

/**
 * @typedef {Object} TransferResponse
 * @property {string} txn_id - Transaction id
 * @property {boolean} success - Whether the transfer succeeded
 * @property {string} chain - Chain key (lowercase)
 * @property {string} origin_symbol - Symbol of the asset sent
 * @property {number} origin_amount - Amount sent in ether
 * @property {string} gas - Gas used/cost
 * @property {string} timestamp - Timestamp
 * @property {string} destination_address - Destination address
 */

/**
 * @typedef {Object} GetChainsOptions
 * @property {boolean} [isEvm=true] - If true return EVM chains; if false return non-EVM chains
 * @property {boolean} [isTestnet=false] - If true (and isEvm) return testnet EVM chains only; ignored when isEvm is false
 */

/**
 * @typedef {Object} ChainInfoItem
 * @property {string} chain - Chain key (lowercase identifier for API calls, e.g. "ethereum", "bnbsmartchain")
 * @property {string} display_name - Human-readable chain name for UI
 * @property {string} token - Asset/token symbol
 */

/**
 * @typedef {Object} GetChainsResponse
 * @property {ChainInfoItem[]} chains - List of supported chains with chain, display_name, and token
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
 * @param {string} context - Label for the request (e.g. "get_api_key_balance")
 * @returns {Promise<any>} Parsed JSON body on success
 * @throws {Error} With server error message when res.ok is false (body message included when present)
 */
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

export class LitTransferApiClient {
  /**
   * @param {Object} options
   * @param {string} [options.baseUrl='http://localhost:8000'] - Base URL of the API
   */
  constructor({ baseUrl = 'http://localhost:8000' } = {}) {
    const base = baseUrl.replace(/\/$/, '');
    this.baseUrl = `${base}/transfer/v1`;
  }

  /**
   * GET /transfer/v1/get_api_key_balance/<api_key>/<chain>
   * Gets balance for the wallet identified by the API key on the given chain.
   * @param {string} apiKey - Hex-encoded API key (from getApiKey)
   * @param {string} chain - Chain key (lowercase, e.g. "ethereum", "yellowstone"); use ChainInfoItem.chain from get_chains
   * @returns {Promise<GetBalanceResponse>} { address, balance, chain, symbol }
   */
  async getApiKeyBalance(apiKey, chain) {
    const res = await fetch(
      `${this.baseUrl}/get_api_key_balance/${encodeURIComponent(apiKey)}/${encodeURIComponent(chain)}`
    );
    return parseResponse(res, 'get_api_key_balance');
  }

  /**
   * GET /transfer/v1/get_pkp_balance/<pkp_public_key>/<chain>
   * Gets balance for the PKP (programmable key pair) address on the given chain.
   * @param {string} pkpPublicKey - PKP public key
   * @param {string} chain - Chain key (lowercase, e.g. "ethereum", "solana"); use ChainInfoItem.chain from get_chains
   * @returns {Promise<GetBalanceResponse>} { address, balance, chain, symbol }
   */
  async getPkpBalance(pkpPublicKey, chain) {
    const res = await fetch(
      `${this.baseUrl}/get_pkp_balance/${encodeURIComponent(pkpPublicKey)}/${encodeURIComponent(chain)}`
    );
    return parseResponse(res, 'get_pkp_balance');
  }

  /**
   * GET /transfer/v1/get_address_balance/<address>/<chain>
   * Gets balance for an arbitrary address on the given chain.
   * @param {string} address - Wallet or contract address (e.g. 0x... for EVM)
   * @param {string} chain - Chain key (lowercase, e.g. "ethereum", "solana"); use ChainInfoItem.chain from get_chains
   * @returns {Promise<GetBalanceResponse>} { address, balance, chain, symbol }
   */
  async getAddressBalance(address, chain) {
    const res = await fetch(
      `${this.baseUrl}/get_address_balance/${encodeURIComponent(address)}/${encodeURIComponent(chain)}`
    );
    return parseResponse(res, 'get_address_balance');
  }

  /**
   * GET /transfer/v1/get_chains?is_evm=&is_testnet=
   * Returns the list of supported chains (EVM, non-EVM, or testnet EVM) with chain key, display_name, and token.
   * Use chain for API calls (getPkpBalance, send, etc.); use display_name for UI labels.
   * @param {GetChainsOptions} [options] - { isEvm, isTestnet }; default { isEvm: true, isTestnet: false }
   * @returns {Promise<GetChainsResponse>} { chains: { chain, display_name, token }[] }
   */
  async getAllChains(options = {}) {
    const { isEvm = true, isTestnet = false } = options;
    const params = new URLSearchParams({
      is_evm: String(isEvm),
      is_testnet: String(isTestnet),
    });
    const url = `${this.baseUrl}/get_chains?${params.toString()}`;
    const res = await fetch(url, { method: 'GET' });
    return parseResponse(res, 'get_chains');
  }

  /**
   * POST /transfer/v1/send
   * Sends funds to a destination address on a chain (PKP-signed).
   * @param {TransferOptions} options
   * @returns {Promise<TransferResponse>}
   */
  async send({ apiKey, pkpPublicKey, chain, destinationAddress, amount }) {
    const body = {
      api_key: apiKey,
      pkp_public_key: pkpPublicKey,
      chain,
      destination_address: destinationAddress,
      amount: Number(amount),
    };
    const res = await fetch(`${this.baseUrl}/send`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'send');
  }
}

/**
 * Factory for a transfer client.
 * @param {string} [baseUrl='http://localhost:8000']
 * @returns {LitTransferApiClient}
 */
export function createTransferClient(baseUrl = 'http://localhost:8000') {
  return new LitTransferApiClient({ baseUrl });
}

export default LitTransferApiClient;
