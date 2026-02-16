import * as ops from 'ext:core/ops';
import { Uint8arrays } from 'ext:lit_actions/01_uint8arrays.js';
/**
 * Check if a given IPFS ID is permitted to sign using a given PKP tokenId
 * @name Lit.Actions.isPermittedAction
 * @function isPermittedAction
 * @param {Object} params
 * @param {string} params.tokenId The tokenId to check
 * @param {string} params.ipfsId The IPFS ID of some JS code (a lit action)
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<boolean>} A boolean indicating whether the IPFS ID is permitted to sign using the PKP tokenId
 */
function isPermittedAction({ tokenId, ipfsId, keySetId }) {
  return ops.op_pkp_permissions_is_permitted(
    'isPermittedAction',
    tokenId,
    [ipfsId],
    keySetId
  );
}

/**
 * Check if a given wallet address is permitted to sign using a given PKP tokenId
 * @name Lit.Actions.isPermittedAddress
 * @function isPermittedAddress
 * @param {Object} params
 * @param {string} params.tokenId The tokenId to check
 * @param {string} params.address The wallet address to check
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<boolean>} A boolean indicating whether the wallet address is permitted to sign using the PKP tokenId
 */
function isPermittedAddress({ tokenId, address, keySetId }) {
  return ops.op_pkp_permissions_is_permitted(
    'isPermittedAddress',
    tokenId,
    [address],
    keySetId
  );
}

/**
 * Check if a given auth method is permitted to sign using a given PKP tokenId
 * @name Lit.Actions.isPermittedAuthMethod
 * @function isPermittedAuthMethod
 * @param {Object} params
 * @param {string} params.tokenId The tokenId to check
 * @param {number} params.authMethodType The auth method type.  This is an integer.  This mapping shows the initial set but this set may be expanded over time without updating this contract: https://github.com/LIT-Protocol/LitNodeContracts/blob/main/contracts/PKPPermissions.sol#L25
 * @param {Uint8Array} params.userId The id of the auth method to check expressed as an array of unsigned 8-bit integers (a Uint8Array)
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<boolean>} A boolean indicating whether the auth method is permitted to sign using the PKP tokenId
 */
function isPermittedAuthMethod({
  tokenId,
  authMethodType,
  userId,
  keySetId,
}) {
  return ops.op_pkp_permissions_is_permitted_auth_method(
    tokenId,
    authMethodType,
    new Uint8Array(userId),
    keySetId
  );
}

/**
 * Get the full list of actions that are permitted to sign using a given PKP tokenId
 * @name Lit.Actions.getPermittedActions
 * @function getPermittedActions
 * @param {Object} params
 * @param {string} params.tokenId The tokenId to check
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<Array<string>>} An array of IPFS IDs of lit actions that are permitted to sign using the PKP tokenId
 */
function getPermittedActions({ tokenId, keySetId }) {
  return ops.op_pkp_permissions_get_permitted(
    'getPermittedActions',
    tokenId,
    keySetId
  );
}

/**
 * Get the full list of addresses that are permitted to sign using a given PKP tokenId
 * @name Lit.Actions.getPermittedAddresses
 * @function getPermittedAddresses
 * @param {Object} params
 * @param {string} params.tokenId The tokenId to check
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<Array<string>>} An array of addresses that are permitted to sign using the PKP tokenId
 */
function getPermittedAddresses({ tokenId, keySetId }) {
  return ops.op_pkp_permissions_get_permitted(
    'getPermittedAddresses',
    tokenId,
    keySetId
  );
}

/**
 * Get the full list of auth methods that are permitted to sign using a given PKP tokenId
 * @name Lit.Actions.getPermittedAuthMethods
 * @function getPermittedAuthMethods
 * @param {Object} params
 * @param {string} params.tokenId The tokenId to check
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<Array<Object>>} An array of auth methods that are permitted to sign using the PKP tokenId.  Each auth method is an object with the following properties: auth_method_type, id, and user_pubkey (used for web authn, this is the pubkey of the user's authentication keypair)
 */
function getPermittedAuthMethods({ tokenId, keySetId }) {
  return ops.op_pkp_permissions_get_permitted(
    'getPermittedAuthMethods',
    tokenId,
    keySetId
  );
}

/**
 * Get the permitted auth method scopes for a given PKP tokenId and auth method type + id
 * @name Lit.Actions.getPermittedAuthMethodScopes
 * @function getPermittedAuthMethodScopes
 * @param {Object} params
 * @param {string} params.tokenId The tokenId to check
 * @param {string} params.authMethodType The auth method type to look up
 * @param {Uint8Array} params.userId The id of the auth method to check expressed as an array of unsigned 8-bit integers (a Uint8Array)
 * @param {number} params.maxScopeId The maximum scope id to check.  This is an integer.
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<Array<boolean>>} An array of booleans that define if a given scope id is turned on.  The index of the array is the scope id.  For example, if the array is [true, false, true], then scope ids 0 and 2 are turned on, but scope id 1 is turned off.
 */
function getPermittedAuthMethodScopes({
  tokenId,
  authMethodType,
  userId,
  maxScopeId = 100,
  keySetId,
}) {
  return ops.op_pkp_permissions_get_permitted_auth_method_scopes(
    tokenId,
    authMethodType,
    new Uint8Array(userId),
    maxScopeId,
    keySetId
  );
}

/**
 * Converts a PKP public key to a PKP token ID by hashing it with keccak256
 * @name Lit.Actions.pubkeyToTokenId
 * @function pubkeyToTokenId
 * @param {Object} params
 * @param {string} params.publicKey The public key to convert
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<string>} The token ID as a string
 */
function pubkeyToTokenId({ publicKey, keySetId }) {
  return ops.op_pubkey_to_token_id(publicKey, keySetId);
}

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
 * Ask the Lit Node to sign any data using the ECDSA Algorithm with it's private key share.  The resulting signature share will be returned to the Lit JS SDK which will automatically combine the shares and give you the full signature to use.
 * @name Lit.Actions.signEcdsa
 * @function signEcdsa
 * @param {Object} params
 * @param {Uint8Array} params.toSign The data to sign.  Should be an array of 8-bit integers.
 * @param {string} params.publicKey The public key of the PKP you wish to sign with
 * @param {string} params.sigName You can put any string here.  This is used to identify the signature in the response by the Lit JS SDK.  This is useful if you are signing multiple messages at once.  When you get the final signature out, it will be in an object with this signature name as the key.
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<string>} This function will return the string "success" if it works.  The signature share is returned behind the scenes to the Lit JS SDK which will automatically combine the shares and give you the full signature to use.
 */
function signEcdsa({ toSign, publicKey, sigName, keySetId }) {
  return ops.op_sign_ecdsa(
    new Uint8Array(toSign),
    publicKey,
    sigName,
    keySetId
  );
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
 * @param {string} params.keySetId The key set id to use
 * @name Lit.Actions.sign
 * @function sign
 * @returns {Uint8array} The resulting signature share
 */
function sign({ toSign, publicKey, sigName, signingScheme, keySetId }) {
  return ops.op_sign(
    new Uint8Array(toSign),
    publicKey,
    sigName,
    signingScheme,
    keySetId
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
 * Ask the Lit Node to sign a message using the eth_personalSign algorithm.  The resulting signature share will be returned to the Lit JS SDK which will automatically combine the shares and give you the full signature to use.
 * @name Lit.Actions.ethPersonalSignMessageEcdsa
 * @function ethPersonalSignMessageEcdsa
 * @param {Object} params
 * @param {string} params.message The message to sign.  Should be a string.
 * @param {string} params.publicKey The public key of the PKP you wish to sign with
 * @param {string} params.sigName You can put any string here.  This is used to identify the signature in the response by the Lit JS SDK.  This is useful if you are signing multiple messages at once.  When you get the final signature out, it will be in an object with this signature name as the key.
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<string>} This function will return the string "success" if it works.  The signature share is returned behind the scenes to the Lit JS SDK which will automatically combine the shares and give you the full signature to use.
 */
function ethPersonalSignMessageEcdsa({
  message,
  publicKey,
  sigName,
  keySetId,
}) {
  return ops.op_sign_ecdsa_eth_personal_sign_message(
    uint8arrayFromString(message),
    publicKey,
    sigName,
    keySetId
  );
}

/**
 * Checks a condition using the Lit condition checking engine.  This is the same engine that powers our Access Control product.  You can use this to check any condition that you can express in our condition language.  This is a powerful tool that allows you to build complex conditions that can be checked in a decentralized way.  Visit https://developer.litprotocol.com and click on the "Access Control" section to learn more.
 * @name Lit.Actions.checkConditions
 * @function checkConditions
 * @param {Object} params
 * @param {Array<Object>} params.conditions An array of access control condition objects
 * @param {Object} params.authSig The AuthSig to use for the condition check.  For example, if you were checking for NFT ownership, this AuthSig would be the signature from the NFT owner's wallet.
 * @param {string} params.chain The chain this AuthSig comes from
 * @returns {Promise<boolean>} A boolean indicating whether the condition check passed or failed
 */
function checkConditions({ conditions, authSig, chain }) {
  return ops.op_check_conditions(conditions, authSig, chain);
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
 * Claim a key through a key identifier, the result of the claim will be added to `claim_id`
 * under the `keyId` given.
 * @name Lit.Actions.claimKey
 * @function claimKey
 * @param {Object} params
 * @param {string} params.keyId user id of the claim
 */
function claimKey({ keyId }) {
  return ops.op_claim_key_identifier(keyId);
}

/**
 * Broadcast a message to all connected clients and collect their responses
 * @name Lit.Actions.broadcastAndCollect
 * @function broadcastAndCollect
 * @param {Object} params
 * @param {string} params.name The name of the broadcast
 * @param {string} params.value The value to broadcast
 * @returns {Promise<string>} The collected responses as a json array
 */
function broadcastAndCollect({ name, value }) {
  return ops.op_broadcast_and_collect(name, value);
}

/**
 * Decrypt and combine the provided ciphertext
 *
 * Important Considerations:
 * - Only unified access control conditions are supported. Standard/legacy ACC formats are not accepted.
 *   When specifying EVM contract conditions, use the unified format with `conditionType: "evmContract"`.
 * - Timeouts are commonly caused by nondeterminism in Lit Actions. Ensure your action code is deterministic
 *   (avoid unseeded randomness, time-based logic, race conditions, or non-deterministic external calls).
 *
 * @name Lit.Actions.decryptAndCombine
 * @function decryptAndCombine
 * @param {Object} params
 * @param {Array<Object>} params.accessControlConditions The access control conditions
 * @param {string} params.ciphertext The ciphertext to decrypt
 * @param {string} params.dataToEncryptHash The hash of the data to encrypt
 * @param {Object} params.authSig The auth signature
 * @param {string} params.chain The chain
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<string>} The decrypted and combined data
 */
function decryptAndCombine({
  accessControlConditions,
  ciphertext,
  dataToEncryptHash,
  authSig,
  chain,
  keySetId,
}) {
  return ops.op_decrypt_and_combine(
    accessControlConditions,
    ciphertext,
    dataToEncryptHash,
    authSig,
    chain,
    keySetId
  );
}

/**
 * Decrypt to a single node
 * @name Lit.Actions.decryptToSingleNode
 * @function decryptToSingleNode
 * @param {Object} params
 * @param {Array<Object>} params.accessControlConditions The access control conditions
 * @param {string} params.ciphertext The ciphertext to decrypt
 * @param {string} params.dataToEncryptHash The hash of the data to encrypt
 * @param {Object} params.authSig The auth signature
 * @param {string} params.chain The chain
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<string>} The decrypted data
 */
function decryptToSingleNode({
  accessControlConditions,
  ciphertext,
  dataToEncryptHash,
  authSig,
  chain,
  keySetId,
}) {
  return ops.op_decrypt_to_single_node(
    accessControlConditions,
    ciphertext,
    dataToEncryptHash,
    authSig,
    chain,
    keySetId
  );
}

/**
 * Sign with ECDSA and automatically combine signature shares from all nodes into a complete signature
 * @name Lit.Actions.signAndCombineEcdsa
 * @function signAndCombineEcdsa
 * @param {Object} params
 * @param {Uint8Array} params.toSign The message to sign
 * @param {string} params.publicKey The public key of the PKP
 * @param {string} params.sigName The name of the signature
 * @param {string} params.keySetId The key set id to use
 * @returns {Promise<Uint8Array>} The resulting combined signature
 */
function signAndCombineEcdsa({
  toSign,
  publicKey,
  sigName,
  keySetId,
}) {
  return ops.op_sign_and_combine_ecdsa(
    new Uint8Array(toSign),
    publicKey,
    sigName,
    keySetId
  );
}

/**
 * Sign with any signing scheme and automatically combine signature shares from all nodes into a complete signature
 * @name Lit.Actions.signAndCombine
 * @function signAndCombine
 * @param {Object} params
 * @param {Uint8Array} params.toSign The message to sign
 * @param {string} params.publicKey The public key of the PKP
 * @param {string} params.sigName The name of the signature
 * @param {string} params.signingScheme The signing scheme. Must be one of:
 * @param {string} params.keySetId The key set id to use
 *   "EcdsaK256Sha256", "EcdsaP256Sha256", "EcdsaP384Sha384",
 *   "SchnorrEd25519Sha512", "SchnorrK256Sha256", "SchnorrP256Sha256", "SchnorrP384Sha384",
 *   "SchnorrRistretto25519Sha512", "SchnorrEd448Shake256", "SchnorrRedJubjubBlake2b512",
 *   "SchnorrK256Taproot", "SchnorrRedDecaf377Blake2b512", "SchnorrkelSubstrate",
 *   "Bls12381G1ProofOfPossession"
 * @returns {Promise<Uint8Array>} The resulting combined signature
 */
function signAndCombine({
  toSign,
  publicKey,
  sigName,
  signingScheme,
  keySetId,
}) {
  return ops.op_sign_and_combine(
    new Uint8Array(toSign),
    publicKey,
    sigName,
    signingScheme,
    keySetId
  );
}

/**
 * Run a function only once across all nodes using leader election
 * @name Lit.Actions.runOnce
 * @function runOnce
 * @param {Object} params
 * @param {boolean} params.waitForResponse Whether to wait for a response or not - if false, the function will return immediately
 * @param {string} params.name Optional name for this runOnce invocation
 * @param {Function} async_fn The async function to run on the leader node
 * @returns {Promise<string>} The response from the function if waitForResponse is true
 */
async function runOnce({ waitForResponse, name }, async_fn) {
  const bc_id = name || 'default_bc_id';
  const is_leader = await ops.op_is_leader();

  if (is_leader) {
    let response = '';

    try {
      response = await async_fn();
    } catch (e) {
      console.error('Error running function:', e);
      response = '[ERROR]';
    }

    try {
      response = response.toString();
    } catch (e) {
      console.error('Error converting response to string:', e);
      response = '';
    }

    if (waitForResponse) {
      ops.op_p2p_broadcast(bc_id, response);
    }

    return response;
  }

  if (waitForResponse) {
    let val = await ops.op_p2p_collect_from_leader(bc_id);

    return val;
  }
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
  isPermittedAction,
  isPermittedAddress,
  isPermittedAuthMethod,

  getPermittedActions,
  getPermittedAddresses,
  getPermittedAuthMethods,
  getPermittedAuthMethodScopes,
  getLatestNonce,
  signEcdsa,
  sign,
  signAsAction,
  getActionPublicKey,
  verifyActionSignature,
  ethPersonalSignMessageEcdsa,

  claimKey,

  checkConditions,
  setResponse,
  call,
  callContract,
  pubkeyToTokenId,
  uint8arrayToString,
  uint8arrayFromString,
  aesDecrypt,

  broadcastAndCollect,
  decryptAndCombine,
  signAndCombineEcdsa,
  signAndCombine,
  runOnce,
  getRpcUrl,
  encrypt,
  decryptToSingleNode,
};
