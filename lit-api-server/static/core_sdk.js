/**
 * Lit Node Simple API - JavaScript Core SDK
 *
 * Wrapper for the v1 API endpoints defined in lit-api-server.
 * Types match core/v1/models (request.rs, response.rs) and core/v1/endpoints.rs.
 * Routes are mounted at /core/v1/ (see src/main.rs).
 */

// --- Request types (match core/v1/models/request.rs) ---

/** Default signing scheme for signWithPkp (secp256k1 + SHA-256). */
export const SIGNING_SCHEME_ECDSA_K256_SHA256 = 'EcdsaK256Sha256';

/**
 * @typedef {Object} SignWithPkpOptions
 * @property {string} apiKey - Hex-encoded API key (from getApiKey)
 * @property {string} pkpPublicKey - PKP public key
 * @property {string} message - Message to sign
 * @property {string} [signingScheme='EcdsaK256Sha256'] - Signing scheme (use SIGNING_SCHEME_ECDSA_K256_SHA256)
 */

/**
 * @typedef {Object} LitActionOptions
 * @property {string} apiKey - Hex-encoded API key (from getApiKey)
 * @property {string} code - Lit action JavaScript code
 * @property {Object} [jsParams] - Optional JSON params passed to the lit action
 */

/** Share type enum (response.rs ShareType). */
export const SHARE_TYPE_ECDSA = 'Ecdsa';
export const SHARE_TYPE_FROST = 'Frost';
export const SHARE_TYPE_BLS = 'Bls';

/**
 * Single signature share (response.rs SignatureShare).
 * @typedef {Object} SignatureShare
 * @property {string} share_id
 * @property {string} peer_id
 * @property {string} signature_share
 */

// --- Response types (match core/v1/models/response.rs) ---

/**
 * @typedef {Object} GetApiKeyResponse
 * @property {string} api_key - Hex-encoded API key
 * @property {string} wallet_address - Wallet address for the API key
 */

/**
 * @typedef {Object} CreateWalletResponse
 * @property {string} wallet_address - Created wallet address
 */

/**
 * Sign-with-PKP response (response.rs SignWithPkpResponse).
 * @typedef {Object} SignWithPkpResponse
 * @property {string} signing_scheme - Signing scheme (e.g. EcdsaK256Sha256)
 * @property {string} signed_digest - Signed digest (hex)
 * @property {string} public_key - Public key (hex)
 * @property {string} share_type - 'Ecdsa' | 'Frost' | 'Bls'
 * @property {string} [big_r] - ECDSA big R (optional)
 * @property {string} [compressed_public_key] - Compressed public key (optional)
 * @property {string} [verifying_share] - Verifying share (optional)
 * @property {string} [signing_commitments] - Signing commitments (optional)
 * @property {SignatureShare[]} shares - Signature shares (share_id, peer_id, signature_share)
 */

/**
 * @typedef {Object} LitActionResponse - Lit action execution result (single response from /lit_action)
 * @property {SignWithPkpResponse[]} signatures - Signing results from the action
 * @property {string} response - Action response payload
 * @property {string} logs - Action logs
 * @property {boolean} has_error - Whether the action reported an error
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
   * GET /core/v1/get_api_key
   * Generates and returns a new API key (hex-encoded wallet secret).
   * @returns {Promise<GetApiKeyResponse>}
   */
  async getApiKey() {
    const res = await fetch(`${this.baseUrl}/get_api_key`);
    return parseResponse(res, 'get_api_key');
  }

  /**
   * GET /core/v1/create_wallet/<api_key>
   * Creates a wallet for the given API key and returns the wallet address.
   * @param {string} apiKey - Hex-encoded API key (from getApiKey)
   * @returns {Promise<CreateWalletResponse>}
   */
  async createWallet(apiKey) {
    const res = await fetch(`${this.baseUrl}/create_wallet/${encodeURIComponent(apiKey)}`);
    return parseResponse(res, 'create_wallet');
  }

  /**
   * POST /core/v1/sign_with_pkp
   * Signs a message with the given PKP using the provided API key.
   * Uses EcdsaK256Sha256 signing scheme by default.
   * @param {SignWithPkpOptions} options
   * @returns {Promise<SignWithPkpResponse>} { signing_scheme, signed_digest, public_key, share_type, shares, ... }
   */
  async signWithPkp({ apiKey, pkpPublicKey, message, signingScheme = SIGNING_SCHEME_ECDSA_K256_SHA256 }) {
    const body = {
      api_key: apiKey,
      pkp_public_key: pkpPublicKey,
      message,
      signing_scheme: signingScheme,
    };
    const res = await fetch(`${this.baseUrl}/sign_with_pkp`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'sign_with_pkp');
  }

  /**
   * POST /core/v1/lit_action
   * Executes a lit action with the given code and optional params.
   * @param {LitActionOptions} options
   * @returns {Promise<LitActionResponse>} { signatures, response, logs, has_error }
   */
  async litAction({ apiKey, code, jsParams }) {
    const body = {
      api_key: apiKey,
      code,
      js_params: jsParams ?? null,
    };
    const res = await fetch(`${this.baseUrl}/lit_action`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'lit_action');
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
