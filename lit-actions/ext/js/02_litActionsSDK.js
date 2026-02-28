import * as ops from 'ext:core/ops';
import { Uint8arrays } from 'ext:lit_actions/01_uint8arrays.js';
/**
 * Gets latest nonce for the given address on a supported chain
 * @name Lit.Actions.getLatestNonce
 * @function getLatestNonce
 * @param {Object} params
 * @param {string} params.address The wallet address for getting the nonce
 * @param {string} params.chain The chain of which the nonce is fetched
 * @returns {Promise<string>} The token ID as a string
 */
function getLatestNonce({ address, chain }) {
  return ops.op_get_latest_nonce(address, chain);
}

/**
 * 
 * Ask the Lit Node to sign any data using the ECDSA Algorithm with it's private key .  The resulting signature  will be returned to the Lit JS SDK which will automatically combine the s and give you the full signature to use.
 * @name Lit.Actions.signEcdsa
 * @function signEcdsa
 * @param {Object} params
 * @param {Uint8Array} params.toSign The message to sign
 * @param {string} params.publicKey The public key of the PKP
 * @param {string} params.sigName The name of the signature
 * @name Lit.Actions.signEcdsa
 * @function signEcdsa
 * @returns {Promise<Uint8Array>} The resulting signature 
 */
function signEcdsa({ toSign, publicKey, sigName}) {
  return sign({ toSign, publicKey, sigName, signingScheme: "EcdsaK256Sha256" });
}

/**
 * @param {Uint8array} toSign the message to sign
 * @param {string} publicKey the public key of the PKP
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
 * @returns {Uint8array} The resulting signature 
 */
function sign({ toSign, publicKey, sigName, signingScheme }) {
  return ops.op_sign(
    new Uint8Array(toSign),
    publicKey,
    sigName,
    signingScheme,
  );
}

/**
 * Sign data using the Lit Action's own cryptographic identity derived from its IPFS CID.
 * This allows actions to sign as themselves (not as a PKP), enabling autonomous agent behavior,
 * action-to-action authentication, and verifiable computation results.
 *
 * The action's keypair is deterministically derived from: keccak256("lit_action_" + actionIpfsCid)
 * The same action IPFS CID always generates the same keypair across all nodes.
 *
 * @name Lit.Actions.signAsAction
 * @function signAsAction
 * @param {Object} params
 * @param {Uint8Array} params.toSign The message to sign as an array of 8-bit integers
 * @param {string} params.sigName The name to identify this signature in the response
 * @param {string} params.signingScheme The signing algorithm to use. Must be one of:
 *   "EcdsaK256Sha256", "EcdsaP256Sha256", "EcdsaP384Sha384",
 *   "SchnorrEd25519Sha512", "SchnorrK256Sha256", "SchnorrP256Sha256", "SchnorrP384Sha384",
 *   "SchnorrRistretto25519Sha512", "SchnorrEd448Shake256", "SchnorrRedJubjubBlake2b512",
 *   "SchnorrK256Taproot", "SchnorrRedDecaf377Blake2b512", "SchnorrkelSubstrate",
 *   "Bls12381G1ProofOfPossession"
 * @returns {Promise<Uint8Array>} The resulting signature that can be verified using verifyActionSignature
 */
function signAsAction({ toSign, sigName, signingScheme }) {
  return ops.op_sign_as_action(new Uint8Array(toSign), sigName, signingScheme);
}

/**
 * Get the public key for a Lit Action's cryptographic identity.
 * This can be used to verify signatures created by signAsAction, or to get the public key
 * of any action (including actions you didn't create) for verification purposes.
 *
 * The public key is deterministically derived from: keccak256("lit_action_" + actionIpfsCid)
 * and will always be the same for a given action IPFS CID and signing scheme.
 *
 * @name Lit.Actions.getActionPublicKey
 * @function getActionPublicKey
 * @param {Object} params
 * @param {string} params.signingScheme The signing algorithm. Must be one of:
 *   "EcdsaK256Sha256", "EcdsaP256Sha256", "EcdsaP384Sha384",
 *   "SchnorrEd25519Sha512", "SchnorrK256Sha256", "SchnorrP256Sha256", "SchnorrP384Sha384",
 *   "SchnorrRistretto25519Sha512", "SchnorrEd448Shake256", "SchnorrRedJubjubBlake2b512",
 *   "SchnorrK256Taproot", "SchnorrRedDecaf377Blake2b512", "SchnorrkelSubstrate",
 *   "Bls12381G1ProofOfPossession"
 * @param {string} params.actionIpfsCid The IPFS CID of the Lit Action
 * @returns {Promise<Uint8Array>} The public key for the action
 */
function getActionPublicKey({ signingScheme, actionIpfsCid }) {
  return ops.op_get_action_public_key(signingScheme, actionIpfsCid);
}

/**
 * Verify that a signature was created by a specific Lit Action using signAsAction.
 * This enables action-to-action authentication, verifiable computation, and building trust chains
 * between actions without requiring PKP ownership.
 *
 * @name Lit.Actions.verifyActionSignature
 * @function verifyActionSignature
 * @param {Object} params
 * @param {string} params.signingScheme The signing algorithm. Must be one of:
 *   "EcdsaK256Sha256", "EcdsaP256Sha256", "EcdsaP384Sha384",
 *   "SchnorrEd25519Sha512", "SchnorrK256Sha256", "SchnorrP256Sha256", "SchnorrP384Sha384",
 *   "SchnorrRistretto25519Sha512", "SchnorrEd448Shake256", "SchnorrRedJubjubBlake2b512",
 *   "SchnorrK256Taproot", "SchnorrRedDecaf377Blake2b512", "SchnorrkelSubstrate",
 *   "Bls12381G1ProofOfPossession"
 * @param {string} params.actionIpfsCid The IPFS CID of the Lit Action that should have created the signature
 * @param {Uint8Array} params.toSign The message that was signed
 * @param {string} params.signOutput The signature output from signAsAction (as a string)
 * @returns {Promise<boolean>} true if the signature was created by the specified action, false otherwise
 */
function verifyActionSignature({
  signingScheme,
  actionIpfsCid,
  toSign,
  signOutput,
}) {
  return ops.op_verify_action_signature(
    signingScheme,
    actionIpfsCid,
    new Uint8Array(toSign),
    signOutput
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
 * Convert a Uint8Array to a string.  This is a re-export of this function: https://www.npmjs.com/package/uint8arrays#tostringarray-encoding--utf8
 * @name Lit.Actions.uint8arrayToString
 * @function uint8arrayToString
 * @param {Uint8Array} array The Uint8Array to convert
 * @param {string} encoding The encoding to use.  Defaults to "utf8"
 * @returns {string} The string representation of the Uint8Array
 */
function uint8arrayToString(...args) {
  return Uint8arrays.toString(...args);
}

/**
 * Convert a string to a Uint8Array.  This is a re-export of this function: https://www.npmjs.com/package/uint8arrays#fromstringstring-encoding--utf8
 * @name Lit.Actions.uint8arrayFromString
 * @function uint8arrayFromString
 * @param {string} string The string to convert
 * @param {string} encoding The encoding to use.  Defaults to "utf8"
 * @returns {Uint8Array} The Uint8Array representation of the string
 */
function uint8arrayFromString(...args) {
  return Uint8arrays.fromString(...args);
}

/**
 * Decrypt data using AES with a symmetric key
 * @name Lit.Actions.aesDecrypt
 * @function aesDecrypt
 * @param {Object} params
 * @param {Uint8Array} params.symmetricKey The AES symmetric key
 * @param {Uint8Array} params.ciphertext The ciphertext to decrypt
 * @returns {Promise<string>} The decrypted plaintext
 */
function aesDecrypt({ symmetricKey, ciphertext }) {
  return ops.op_aes_decrypt(
    new Uint8Array(symmetricKey),
    new Uint8Array(ciphertext)
  );
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
 * @name Lit.Actions.encrypt
 * @function encrypt
 * @param {Object} params
 * @param {Array<Object>} params.accessControlConditions The access control conditions that must be met to decrypt
 * @param {string} params.to_encrypt The message to encrypt
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<{ciphertext: string, dataToEncryptHash: string}>} An object containing the ciphertext and the hash of the data that was encrypted
 */
function encrypt({
  accessControlConditions,
  to_encrypt,
  keySetId,
}) {
  return ops.op_encrypt_bls(accessControlConditions, to_encrypt, keySetId);
}
globalThis.LitActions = {
  getLatestNonce,
  sign,
  signEcdsa,
  signAsAction,
  getActionPublicKey,
  verifyActionSignature,
  setResponse,
  call,
  callContract,
  uint8arrayToString,
  uint8arrayFromString,
  aesDecrypt,
  getRpcUrl,
  encrypt,
};
