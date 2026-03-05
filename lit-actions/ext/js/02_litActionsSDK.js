import * as ops from 'ext:core/ops';
/**
 * Ask the Lit Node to sign any data using the ECDSA Algorithm with its private key. The resulting signature will be returned to the Lit JS SDK which will automatically combine the shares and give you the full signature to use.
 * @name Lit.Actions.signEcdsa
 * @function signEcdsa
 * @param {Object} params
 * @param {Uint8Array} params.toSign The message to sign
 * @param {string} params.pkpId The ID of the PKP
 * @param {string} params.sigName The name of the signature
 * @returns {Promise<Uint8Array>} The resulting signature
 */
function signEcdsa({ toSign, pkpId, sigName}) {
  return sign({ toSign, pkpId, sigName, signingScheme: "EcdsaK256Sha256" });
}

/**
 * @param {Uint8array} toSign the message to sign
 * @param {string} pkpId the ID of the PKP
 * @param {string} sigName the name of the signature
 * @param {string} signingScheme the name of the signing scheme
 *   one of the following
 *   "EcdsaK256Sha256"
 *   "EcdsaP256Sha256"
 *   "EcdsaP384Sha384"
 *   "SchnorrEd25519Sha512"
 *   "SchnorrK256Sha256"
 *   "SchnorrP256Sha256"
 *   "SchnorrP384Sha384"
 *   "SchnorrRistretto25519Sha512"
 *   "SchnorrEd448Shake256"
 *   "SchnorrRedJubjubBlake2b512"
 *   "SchnorrK256Taproot"
 *   "SchnorrRedDecaf377Blake2b512"
 *   "SchnorrkelSubstrate"
 *   "Bls12381G1ProofOfPossession"
 * @name Lit.Actions.sign
 * @function sign
 * @returns {Uint8Array} The resulting signature 
 */
function sign({ toSign, pkpId, sigName, signingScheme }) {
  return ops.op_sign(
    new Uint8Array(toSign),
    pkpId,
    sigName,
    signingScheme,
  );
}

/**
 * Set the response returned to the client
 * @name Lit.Actions.setResponse
 * @function setResponse
 * @param {Object} params
 * @param {string} params.response The response to send to the client.  You can put any string here, like you could use JSON.stringify on a JS object and send it here.
 */
function setResponse({ response }) {
  return ops.op_set_response(response);
}

/**
 * Call a child Lit Action
 * @name Lit.Actions.call
 * @function call
 * @param {Object} params
 * @param {string} params.ipfsId The IPFS ID of the Lit Action to call
 * @param {Object=} params.params Optional parameters to pass to the child Lit Action
 * @returns {Promise<string>} The response from the child Lit Action.  Note that any signatures performed by the child Lit Action will be automatically combined and returned with the parent Lit Action to the Lit JS SDK client.
 */
function call({ ipfsId, params }) {
  return ops.op_call_child(ipfsId, params);
}

/**
 * Call a smart contract
 * @name Lit.Actions.callContract
 * @function callContract
 * @param {Object} params
 * @param {string} params.chain The name of the chain to use.  Check out the lit docs "Supported Blockchains" page to find the name.  For example, "ethereum"
 * @param {string} params.txn The RLP Encoded txn, as a hex string
 * @returns {Promise<string>} The response from calling the contract
 */
function callContract({ chain, txn }) {
  return ops.op_call_contract(chain, txn);
}

/**
 * Convert a Uint8Array to a string.
 * @name Lit.Actions.uint8arrayToString
 * @function uint8arrayToString
 * @param {Uint8Array} array The Uint8Array to convert
 * @param {string} [encoding='utf8'] The encoding to use (only 'utf8' is supported)
 * @returns {string} The string representation of the Uint8Array
 * @throws {Error} If a non-utf8 encoding is specified
 */
function uint8arrayToString(array, encoding) {
  const enc = encoding || 'utf8';
  if (enc !== 'utf8') throw new Error(`Unsupported encoding: '${enc}'. Only 'utf8' is supported.`);
  return new TextDecoder('utf-8').decode(array);
}

/**
 * Convert a string to a Uint8Array.
 * @name Lit.Actions.uint8arrayFromString
 * @function uint8arrayFromString
 * @param {string} string The string to convert
 * @param {string} [encoding='utf8'] The encoding to use (only 'utf8' is supported)
 * @returns {Uint8Array} The Uint8Array representation of the string
 * @throws {Error} If a non-utf8 encoding is specified
 */
function uint8arrayFromString(string, encoding) {
  const enc = encoding || 'utf8';
  if (enc !== 'utf8') throw new Error(`Unsupported encoding: '${enc}'. Only 'utf8' is supported.`);
  return new Uint8Array(new TextEncoder().encode(string));
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
 * Get the RPC URL for a given blockchain
 * @name Lit.Actions.getRpcUrl
 * @function getRpcUrl
 * @param {Object} params
 * @param {string} params.chain The chain to get the RPC URL for
 * @returns {Promise<string>} The RPC URL for the chain
 */
function getRpcUrl({ chain }) {
  return ops.op_get_rpc_url(chain);
}

/**
 * Encrypt data using BLS encryption with access control conditions
 * @name Lit.Actions.encrypt_bls
 * @function encrypt_bls
 * @param {Object} params
 * @param {Array<Object>} params.accessControlConditions The access control conditions that must be met to decrypt
 * @param {string} params.to_encrypt The message to encrypt
 * @returns {Promise<{ciphertext: string, dataToEncryptHash: string}>} An object containing the ciphertext and the hash of the data that was encrypted
 */
function encrypt_bls({
  accessControlConditions,
  to_encrypt,  
}) {
  return ops.op_encrypt_bls(accessControlConditions, to_encrypt, "");
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

globalThis.LitActions = {
  Encrypt,
  Decrypt,
  sign,
  signEcdsa,
  setResponse,
  call,
  callContract,
  uint8arrayToString,
  uint8arrayFromString,
};
