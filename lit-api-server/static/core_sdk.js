/**
 * Lit Node Simple API - JavaScript Core SDK
 *
 * Wrapper for the v1 API endpoints defined in lit-node-simple-api.
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

/**
 * @typedef {Object} EncryptOptions
 * @property {string} apiKey - Hex-encoded API key (from getApiKey)
 * @property {string} message - Plaintext message to encrypt
 */

/**
 * @typedef {Object} DecryptOptions
 * @property {string} apiKey - Hex-encoded API key (from getApiKey)
 * @property {string} ciphertext - Base64-encoded ciphertext (from encrypt)
 * @property {string} dataToEncryptHash - Hex-encoded SHA-256 hash of the original plaintext (from encrypt response)
 */

/**
 * @typedef {Object} CombineSignatureSharesOptions
 * @property {string} apiKey - Hex-encoded API key (from getApiKey)
 * @property {SignWithPkpResponse} shareData - Full signing response from signWithPkp (pass the whole response object)
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
 * @typedef {Object} HandshakeResponse
 * @property {string[]} responses - Handshake responses from validators
 */

/**
 * @typedef {Object} MintPkpResponse
 * @property {string} pkp_public_key - Minted PKP public key
 */

/**
 * Sign-with-PKP response (response.rs SignWithPkpResponse). Pass the whole object to combineSignatureShares as shareData.
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
 * Single signing result within a lit action (each item is SignWithPkpResponse).
 * @typedef {SignWithPkpResponse} SignWithPkpResponseItem
 */

/**
 * @typedef {Object} LitActionResponse - Single lit action execution result
 * @property {SignWithPkpResponse[]} signatures - Signing results from the action
 * @property {string} response - Action response payload
 * @property {string} logs - Action logs
 * @property {boolean} has_error - Whether the action reported an error
 */

/**
 * @typedef {Object} LitActionResponses - Top-level lit_action endpoint response
 * @property {LitActionResponse[]} responses - One entry per execution (e.g. per node)
 */

/**
 * @typedef {Object} EncryptResponse
 * @property {string} ciphertext - Base64-encoded ciphertext
 * @property {string} data_to_encrypt_hash - Hex-encoded SHA-256 hash of the plaintext (use for decrypt)
 */

/**
 * @typedef {Object} DecryptResponse
 * @property {string} decrypted_text - Decrypted plaintext
 */

/**
 * @typedef {Object} CombineSignatureSharesResponse
 * @property {string} signature - Hex-encoded combined signature
 * @property {string} signed_data - Signed data (hex)
 * @property {string} verifying_key - Verifying key (hex)
 * @property {string} r - ECDSA r component (hex)
 * @property {string} s - ECDSA s component (hex)
 * @property {number} recovery_id - Recovery id byte
 */

/**
 * Get ledger balance response: the raw balance value (U256 serialized as string or number).
 * @typedef {string|number} GetLedgerBalanceResponse
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
   * GET /core/v1/handshake
   * Performs handshake with validators and returns their responses.
   * @returns {Promise<HandshakeResponse>}
   */
  async handshake() {
    const res = await fetch(`${this.baseUrl}/handshake`);
    return parseResponse(res, 'handshake');
  }

  /**
   * GET /core/v1/mint_pkp/<api_key>
   * Mints a new PKP for the wallet identified by the API key.
   * @param {string} apiKey - Hex-encoded API key (from getApiKey)
   * @returns {Promise<MintPkpResponse>}
   */
  async mintPkp(apiKey) {
    const res = await fetch(`${this.baseUrl}/mint_pkp/${encodeURIComponent(apiKey)}`);
    return parseResponse(res, 'mint_pkp');
  }

  /**
   * GET /core/v1/get_ledger_balance/<api_key>
   * Returns the wallet ledger balance for the API key (inquiry balance).
   * @param {string} apiKey - Hex-encoded API key (from getApiKey)
   * @returns {Promise<GetLedgerBalanceResponse>} Balance value (string or number, U256)
   */
  async getLedgerBalance(apiKey) {
    const res = await fetch(`${this.baseUrl}/get_ledger_balance/${encodeURIComponent(apiKey)}`);
    return parseResponse(res, 'get_ledger_balance');
  }

  /**
   * POST /core/v1/sign_with_pkp
   * Signs a message with the given PKP using the provided API key.
   * Uses EcdsaK256Sha256 signing scheme by default. Pass the full response to combineSignatureShares as shareData.
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
   * @returns {Promise<LitActionResponses>} { responses: LitActionResponse[] }
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

  /**
   * POST /core/v1/encrypt
   * Encrypts a message (time-lock encryption) for the wallet identified by the API key.
   * @param {EncryptOptions} options
   * @returns {Promise<EncryptResponse>} { ciphertext, data_to_encrypt_hash }
   */
  async encrypt({ apiKey, message }) {
    const body = { api_key: apiKey, message };
    const res = await fetch(`${this.baseUrl}/encrypt`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'encrypt');
  }

  /**
   * POST /core/v1/decrypt
   * Decrypts ciphertext (server fetches decryption shares from nodes).
   * @param {DecryptOptions} options
   * @returns {Promise<DecryptResponse>} { decrypted_text }
   */
  async decrypt({ apiKey, ciphertext, dataToEncryptHash }) {
    const body = {
      api_key: apiKey,
      ciphertext,
      data_to_encrypt_hash: dataToEncryptHash,
    };
    const res = await fetch(`${this.baseUrl}/decrypt`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'decrypt');
  }

  /**
   * POST /core/v1/combine_signature_shares
   * Combines signature shares. Pass the full SignWithPkpResponse from signWithPkp as shareData.
   * @param {CombineSignatureSharesOptions} options
   * @returns {Promise<CombineSignatureSharesResponse>} { signature, signed_data, verifying_key, r, s, recovery_id }
   */
  async combineSignatureShares({ apiKey, shareData }) {
    const body = { api_key: apiKey, share_date: shareData };
    const res = await fetch(`${this.baseUrl}/combine_signature_shares`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'combine_signature_shares');
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
