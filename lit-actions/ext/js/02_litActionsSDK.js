import * as ops from 'ext:core/ops';
/**
 * Set the response returned to the client
 * @name Lit.Actions.setResponse
 * @function setResponse
 * @param {Object} params
 * @param {*} params.response The response to send to the client. If this is not a string, it will be JSON-encoded before being sent. A value of undefined is encoded as null.
 */
function setResponse({ response }) {
  const stringifiedResponse =
    typeof response === 'string'
      ? response
      : JSON.stringify(response === undefined ? null : response);
  return ops.op_set_response(stringifiedResponse);
}

/**
 * Decrypt data using AES with a symmetric key
 * @name Lit.Actions.Decrypt
 * @function Decrypt
 * @param {Object} params
 * @param {string} params.pkpId The ID of the PKP
 * @param {string} params.ciphertext The ciphertext to decrypt
 * @returns {Promise<string>} The decrypted plaintext
 */
function Decrypt({ pkpId, ciphertext }) {
  return ops.op_aes_decrypt(pkpId, ciphertext);
}

/**
 * @name Lit.Actions.Encrypt
 * @function Encrypt
 * @param {Object} params
 * @param {string} params.pkpId The ID of the PKP
 * @param {string} params.message The message to encrypt
 * @returns {Promise<string>} The ciphertext
 */

function Encrypt({
  pkpId,
  message,
}) {
  return ops.op_aes_encrypt(pkpId, message);
}

/**
 * Get the private key for a PKP wallet
 * @name Lit.Actions.getPrivateKey
 * @function getPrivateKey
 * @param {Object} params
 * @param {string} params.pkpId The ID of the PKP
 * @returns {Promise<string>} The private key secret
 */
function getPrivateKey({ pkpId }) {
  return ops.op_get_private_key(pkpId);
}

/**
 * Get the private key for the currently executing Lit Action
 * @name Lit.Actions.getLitActionPrivateKey
 * @function getLitActionPrivateKey
 * @returns {Promise<string>} The private key secret
 */
function getLitActionPrivateKey() {
  return ops.op_get_lit_action_private_key();
}

/**
 * Get the public key for a Lit Action by IPFS ID
 * @name Lit.Actions.getLitActionPublicKey
 * @function getLitActionPublicKey
 * @param {Object} params
 * @param {string} params.ipfsId The IPFS ID of the Lit Action
 * @returns {Promise<string>} The public key
 */
function getLitActionPublicKey({ ipfsId }) {
  return ops.op_get_lit_action_public_key(ipfsId);
}

/**
 * Get the wallet address for a Lit Action by IPFS ID
 * @name Lit.Actions.getLitActionWalletAddress
 * @function getLitActionWalletAddress
 * @param {Object} params
 * @param {string} params.ipfsId The IPFS ID of the Lit Action
 * @returns {Promise<string>} The wallet address
 */
function getLitActionWalletAddress({ ipfsId }) {
  return ops.op_get_lit_action_wallet_address(ipfsId);
}

/**
 * Log and return details of all modules imported by this Lit Action.
 * Returns an array of objects with the resolved CDN URL and SHA-384 integrity
 * hash for each imported module. The details are also written to the action's
 * console log via the print opCode.
 * @name Lit.Actions.showImportDetails
 * @function showImportDetails
 * @returns {Array<{url: string, hash: string}>} Array of imported module details
 */
function showImportDetails() {
  const json = ops.op_show_import_details();
  return JSON.parse(json);
}

/**
 * Outbound HTTP that can egress through a per-request authenticated proxy, so a
 * Lit Action can reach a venue (e.g. Binance) from a chosen non-US IP even
 * though the enclave's own egress is geo-blocked. TLS to the destination is
 * end-to-end through the proxy's CONNECT tunnel; the proxy never sees venue
 * credentials or payloads. Counts against the same per-action fetch quota as
 * the global `fetch`.
 * @name Lit.Actions.proxiedFetch
 * @function proxiedFetch
 * @param {Object} params
 * @param {string} params.url The absolute URL to request.
 * @param {string} [params.method] HTTP method (default "GET").
 * @param {Object|Array<[string,string]>} [params.headers] Request headers.
 * @param {string} [params.body] Request body.
 * @param {string} [params.proxy] Proxy URL `http(s)://[user:pass@]host:port`. Omit for a direct request.
 * @returns {Promise<{status:number, ok:boolean, headers:Object, text:()=>Promise<string>, json:()=>Promise<any>}>}
 */
async function proxiedFetch({
  url,
  method = 'GET',
  headers = {},
  body = null,
  proxy = null,
} = {}) {
  // Enforce the per-action fetch quota exactly like the wrapped global fetch.
  await ops.op_increment_fetch_count();
  const headerPairs = Array.isArray(headers) ? headers : Object.entries(headers);
  const res = await ops.op_lit_proxied_fetch({
    url,
    method,
    headers: headerPairs,
    body,
    proxy,
  });
  const headerMap = {};
  for (const [k, v] of res.headers) headerMap[String(k).toLowerCase()] = v;
  return {
    status: res.status,
    ok: res.status >= 200 && res.status < 300,
    headers: headerMap,
    text: async () => res.body,
    json: async () => JSON.parse(res.body),
  };
}

/**
 * Send a plain-text notification email, server-mediated (fixed from-domain,
 * per-account quotas, no arbitrary HTML) -- plan D6.
 * @name Lit.Actions.sendEmail
 * @function sendEmail
 * @param {Object} params
 * @param {string} params.to Recipient address.
 * @param {string} params.subject Subject line (server prefixes it to prevent spoofing).
 * @param {string} params.text Plain-text body.
 * @returns {Promise<{accepted: boolean}>}
 */
async function sendEmail({ to, subject, text }) {
  await ops.op_send_email(to, subject, text);
  return { accepted: true };
}

/**
 * Request a human approval over email: issues a single-use approval id and
 * emails a signed approval link. Two-phase by design -- this phase exits, a
 * later invocation calls checkEmailApproval (plan D6).
 * @name Lit.Actions.requestEmailApproval
 * @function requestEmailApproval
 * @param {Object} params
 * @param {string} params.to Approver's email address.
 * @param {string} params.summary What is being approved (shown in the email and on the approval page).
 * @param {string} [params.assurance] "L1" (link click) or "L2" (link + OTP step-up, default -- required for anything that moves funds).
 * @param {number} [params.ttlSec] Approval validity window in seconds (default 3600).
 * @returns {Promise<{approvalId: string, otp?: string, approvalUrl?: string}>} For L2 the OTP must reach the approver out-of-band (e.g. shown in the requesting app) -- email is the notification channel, not the authentication channel.
 */
async function requestEmailApproval({ to, summary, assurance = 'L2', ttlSec = 3600 }) {
  const res = await ops.op_request_email_approval(to, summary, assurance, ttlSec);
  return {
    approvalId: res.approval_id,
    otp: res.otp ?? undefined,
    approvalUrl: res.approval_url ?? undefined,
  };
}

/**
 * Check an approval and verify its attestation IN-TEE against the pinned
 * network attestation key. Returns approved=true only after the runtime has
 * verified the signature, approvalId binding, status, and expiry -- a
 * compromised approval service cannot forge an approval past this check.
 * @name Lit.Actions.checkEmailApproval
 * @function checkEmailApproval
 * @param {Object} params
 * @param {string} params.approvalId The id returned by requestEmailApproval.
 * @returns {Promise<{approved: boolean, status: string, attestation?: string, approver?: string, assurance?: string, approvedAtMs?: number}>}
 */
async function checkEmailApproval({ approvalId }) {
  const res = await ops.op_check_email_approval(approvalId);
  return {
    approved: res.approved,
    status: res.status,
    attestation: res.attestation ?? undefined,
    approver: res.approver ?? undefined,
    assurance: res.assurance ?? undefined,
    approvedAtMs: res.approved_at_ms ?? undefined,
  };
}

globalThis.LitActions = {
  Encrypt,
  Decrypt,
  getPrivateKey,
  getLitActionPrivateKey,
  getLitActionPublicKey,
  getLitActionWalletAddress,
  setResponse,
  showImportDetails,
  proxiedFetch,
  sendEmail,
  requestEmailApproval,
  checkEmailApproval,
};
