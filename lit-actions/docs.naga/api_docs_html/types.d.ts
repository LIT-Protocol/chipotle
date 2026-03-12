export declare namespace Lit {
  export namespace Actions {
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
    function isPermittedAction({
      tokenId,
      ipfsId,
      keySetId,
    }: {
      tokenId: string;
      ipfsId: string;
      keySetId: string;
    }): Promise<boolean>;
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
    function isPermittedAddress({
      tokenId,
      address,
      keySetId,
    }: {
      tokenId: string;
      address: string;
      keySetId: string;
    }): Promise<boolean>;
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
    }: {
      tokenId: string;
      authMethodType: number;
      userId: Uint8Array;
      keySetId: string;
    }): Promise<boolean>;
    /**
     * Get the full list of actions that are permitted to sign using a given PKP tokenId
     * @name Lit.Actions.getPermittedActions
     * @function getPermittedActions
     * @param {Object} params
     * @param {string} params.tokenId The tokenId to check
     * @param {string} params.keySetId The key set id to use
     * @returns {Promise<Array<string>>} An array of IPFS IDs of lit actions that are permitted to sign using the PKP tokenId
     */
    function getPermittedActions({
      tokenId,
      keySetId,
    }: {
      tokenId: string;
      keySetId: string;
    }): Promise<Array<string>>;
    /**
     * Get the full list of addresses that are permitted to sign using a given PKP tokenId
     * @name Lit.Actions.getPermittedAddresses
     * @function getPermittedAddresses
     * @param {Object} params
     * @param {string} params.tokenId The tokenId to check
     * @param {string} params.keySetId The key set id to use
     * @returns {Promise<Array<string>>} An array of addresses that are permitted to sign using the PKP tokenId
     */
    function getPermittedAddresses({
      tokenId,
      keySetId,
    }: {
      tokenId: string;
      keySetId: string;
    }): Promise<Array<string>>;
    /**
     * Get the full list of auth methods that are permitted to sign using a given PKP tokenId
     * @name Lit.Actions.getPermittedAuthMethods
     * @function getPermittedAuthMethods
     * @param {Object} params
     * @param {string} params.tokenId The tokenId to check
     * @param {string} params.keySetId The key set id to use
     * @returns {Promise<Array<Object>>} An array of auth methods that are permitted to sign using the PKP tokenId.  Each auth method is an object with the following properties: auth_method_type, id, and user_pubkey (used for web authn, this is the pubkey of the user's authentication keypair)
     */
    function getPermittedAuthMethods({
      tokenId,
      keySetId,
    }: {
      tokenId: string;
      keySetId: string;
    }): Promise<Array<any>>;
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
      maxScopeId,
      keySetId,
    }: {
      tokenId: string;
      authMethodType: string;
      userId: Uint8Array;
      maxScopeId: number;
      keySetId: string;
    }): Promise<Array<boolean>>;
    /**
     * Converts a PKP public key to a PKP token ID by hashing it with keccak256
     * @name Lit.Actions.pubkeyToTokenId
     * @function pubkeyToTokenId
     * @param {Object} params
     * @param {string} params.publicKey The public key to convert
     * @param {string} params.keySetId The key set id to use
     * @returns {Promise<string>} The token ID as a string
     */
    function pubkeyToTokenId({
      publicKey,
      keySetId,
    }: {
      publicKey: string;
      keySetId: string;
    }): Promise<string>;
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
    }: {
      message: string;
      publicKey: string;
      sigName: string;
      keySetId: string;
    }): Promise<string>;
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
    function checkConditions({
      conditions,
      authSig,
      chain,
    }: {
      conditions: Array<any>;
      authSig: any;
      chain: string;
    }): Promise<boolean>;
    /**
     * Set the response returned to the client
     * @name Lit.Actions.setResponse
     * @function setResponse
     * @param {Object} params
     * @param {string} params.response The response to send to the client.  You can put any string here, like you could use JSON.stringify on a JS object and send it here.
     */
    function setResponse({ response }: { response: string }): any;
    /**
     * Convert a Uint8Array to a string.  This is a re-export of this function: https://www.npmjs.com/package/uint8arrays#tostringarray-encoding--utf8
     * @name Lit.Actions.uint8arrayToString
     * @function uint8arrayToString
     * @param {Uint8Array} array The Uint8Array to convert
     * @param {string} encoding The encoding to use.  Defaults to "utf8"
     * @returns {string} The string representation of the Uint8Array
     */
    function uint8arrayToString(...args: any[]): string;
    /**
     * Convert a string to a Uint8Array.  This is a re-export of this function: https://www.npmjs.com/package/uint8arrays#fromstringstring-encoding--utf8
     * @name Lit.Actions.uint8arrayFromString
     * @function uint8arrayFromString
     * @param {string} string The string to convert
     * @param {string} encoding The encoding to use.  Defaults to "utf8"
     * @returns {Uint8Array} The Uint8Array representation of the string
     */
    function uint8arrayFromString(...args: any[]): Uint8Array;
    /**
     * Decrypt data using AES with a symmetric key
     * @name Lit.Actions.aesDecrypt
     * @function aesDecrypt
     * @param {Object} params
     * @param {Uint8Array} params.symmetricKey The AES symmetric key
     * @param {Uint8Array} params.ciphertext The ciphertext to decrypt
     * @returns {Promise<string>} The decrypted plaintext
     */
    function aesDecrypt({
      symmetricKey,
      ciphertext,
    }: {
      symmetricKey: Uint8Array;
      ciphertext: Uint8Array;
    }): Promise<string>;
    /**
     * Claim a key through a key identifier, the result of the claim will be added to `claim_id`
     * under the `keyId` given.
     * @name Lit.Actions.claimKey
     * @function claimKey
     * @param {Object} params
     * @param {string} params.keyId user id of the claim
     */
    function claimKey({ keyId }: { keyId: string }): any;
    /**
     * Broadcast a message to all connected clients and collect their responses
     * @name Lit.Actions.broadcastAndCollect
     * @function broadcastAndCollect
     * @param {Object} params
     * @param {string} params.name The name of the broadcast
     * @param {string} params.value The value to broadcast
     * @returns {Promise<string>} The collected responses as a json array
     */
    function broadcastAndCollect({
      name,
      value,
    }: {
      name: string;
      value: string;
    }): Promise<string>;
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
    }: {
      accessControlConditions: Array<any>;
      ciphertext: string;
      dataToEncryptHash: string;
      authSig: any;
      chain: string;
      keySetId: string;
    }): Promise<string>;
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
    }: {
      accessControlConditions: Array<any>;
      ciphertext: string;
      dataToEncryptHash: string;
      authSig: any;
      chain: string;
      keySetId: string;
    }): Promise<string>;
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
    }: {
      toSign: Uint8Array;
      publicKey: string;
      sigName: string;
      keySetId: string;
    }): Promise<Uint8Array>;
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
    }: {
      toSign: Uint8Array;
      publicKey: string;
      sigName: string;
      signingScheme: string;
      keySetId: string;
    }): Promise<Uint8Array>;
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
    function runOnce(
      {
        waitForResponse,
        name,
      }: {
        waitForResponse: boolean;
        name: string;
      },
      async_fn: Function,
    ): Promise<string>;
  }

  export namespace Auth {
    /**
     * Stack of action IPFS IDs tracking the call hierarchy.
     * When a parent action calls a child action, the child's IPFS ID is pushed onto this stack.
     * @type {Array<string>}
     */
    const actionIpfsIdStack: Array<string>;

    /**
     * The address from the authentication signature.
     * @type {string | null}
     */
    const authSigAddress: string | null;

    /**
     * Array of authentication method contexts.
     * @type {Array<{
     *   userId: string;
     *   appId: string;
     *   authMethodType: number;
     *   lastRetrievedAt: string;
     *   expiration: number;
     *   usedForSignSessionKeyRequest: boolean;
     * }>}
     */
    const authMethodContexts: {
      userId: string;
      appId: string;
      authMethodType: number;
      lastRetrievedAt: string;
      expiration: number;
      usedForSignSessionKeyRequest: boolean;
    }[];

    /**
     * Array of resources from the SIWE message or session signature.
     * @type {Array<string>}
     */
    const resources: Array<string>;

    /**
     * Custom authentication resource string.
     * The template literal type represents a string of the form: "\"\(true,)\\""
     * Example: "\"\(true,exampleValue)\\""
     * @type {string | `"\\(true,${string})\\"`}
     */
    const customAuthResource: string | `"\\(true,${string})\\"`;
  }
}

/**
 * Global reference to Lit.Actions namespace for convenience.
 * This is identical to using Lit.Actions.
 */
declare const LitActions: typeof Lit.Actions;

/**
 * Global reference to Lit.Auth namespace for convenience.
 * This is identical to using Lit.Auth.
 */
declare const LitAuth: typeof Lit.Auth;

/**
 * The ethers.js v5 library for interacting with Ethereum and other EVM chains.
 * Includes utilities for wallets, contracts, providers, and cryptographic operations.
 * See https://docs.ethers.io/v5/ for full documentation.
 *
 * For full type definitions, install: npm install --save-dev ethers@5
 * Then import types with: import type { ethers } from 'ethers';
 */
declare const ethers: typeof import("ethers");
