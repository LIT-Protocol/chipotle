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
 * Derive an Ethereum address and public key from a private key using native Rust ECDSA.
 * ~100x faster than `new ethers.Wallet(key)` for address derivation.
 * @name Lit.Actions.deriveEthAddress
 * @function deriveEthAddress
 * @param {string} privateKeyHex The hex-encoded private key (with or without 0x prefix)
 * @returns {{ address: string, publicKey: string }}
 */
function deriveEthAddress(privateKeyHex) {
  return ops.op_derive_eth_address(privateKeyHex);
}

/**
 * Sign an Ethereum personal message using native Rust ECDSA.
 * @name Lit.Actions.signMessage
 * @function signMessage
 * @param {string} privateKeyHex The hex-encoded private key
 * @param {string} message The message to sign
 * @returns {string} The signature as a hex string
 */
function signMessage(privateKeyHex, message) {
  return ops.op_sign_message(privateKeyHex, message);
}

globalThis.LitActions = {
  Encrypt,
  Decrypt,
  getPrivateKey,
  getLitActionPrivateKey,
  getLitActionPublicKey,
  getLitActionWalletAddress,
  setResponse,
  deriveEthAddress,
  signMessage,
};
