export declare namespace Lit {
  export namespace Actions {
    /**
     * Ask the Lit Node to sign any data using the ECDSA Algorithm with its private key. The resulting signature will be returned to the Lit JS SDK which will automatically combine the shares and give you the full signature to use.
     * @name Lit.Actions.signEcdsa
     * @function signEcdsa
     * @param {Object} params
     * @param {Uint8Array} params.toSign The message to sign
     * @param {string} params.publicKey The public key of the PKP
     * @param {string} params.sigName The name of the signature
     * @returns {Promise<Uint8Array>} The resulting signature
     */
    function signEcdsa({
      toSign,
      publicKey,
      sigName,
    }: {
      toSign: Uint8Array;
      publicKey: string;
      sigName: string;
    }): Promise<Uint8Array>;
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
     * @returns {Uint8Array} The resulting signature
     */
    function sign({
      toSign,
      publicKey,
      sigName,
      signingScheme,
    }: Uint8array): Uint8Array;
    /**
     * Set the response returned to the client
     * @name Lit.Actions.setResponse
     * @function setResponse
     * @param {Object} params
     * @param {string} params.response The response to send to the client.  You can put any string here, like you could use JSON.stringify on a JS object and send it here.
     */
    function setResponse({ response }: { response: string }): any;
    /**
     * Call a child Lit Action
     * @name Lit.Actions.call
     * @function call
     * @param {Object} params
     * @param {string} params.ipfsId The IPFS ID of the Lit Action to call
     * @param {Object=} params.params Optional parameters to pass to the child Lit Action
     * @returns {Promise<string>} The response from the child Lit Action.  Note that any signatures performed by the child Lit Action will be automatically combined and returned with the parent Lit Action to the Lit JS SDK client.
     */
    function call({
      ipfsId,
      params,
    }: {
      ipfsId: string;
      params?: any | undefined;
    }): Promise<string>;
    /**
     * Call a smart contract
     * @name Lit.Actions.callContract
     * @function callContract
     * @param {Object} params
     * @param {string} params.chain The name of the chain to use.  Check out the lit docs "Supported Blockchains" page to find the name.  For example, "ethereum"
     * @param {string} params.txn The RLP Encoded txn, as a hex string
     * @returns {Promise<string>} The response from calling the contract
     */
    function callContract({
      chain,
      txn,
    }: {
      chain: string;
      txn: string;
    }): Promise<string>;
    /**
     * Convert a Uint8Array to a string.
     * @name Lit.Actions.uint8arrayToString
     * @function uint8arrayToString
     * @param {Uint8Array} array The Uint8Array to convert
     * @param {string} [encoding='utf8'] The encoding to use (utf8 supported)
     * @returns {string} The string representation of the Uint8Array
     */
    function uint8arrayToString(array: Uint8Array, encoding?: string): string;
    /**
     * Convert a string to a Uint8Array.
     * @name Lit.Actions.uint8arrayFromString
     * @function uint8arrayFromString
     * @param {string} string The string to convert
     * @param {string} [encoding='utf8'] The encoding to use (utf8 supported)
     * @returns {Uint8Array} The Uint8Array representation of the string
     */
    function uint8arrayFromString(
      string: string,
      encoding?: string,
    ): Uint8Array;
    /**
     * Decrypt data using AES with a symmetric key
     * @name Lit.Actions.Decrypt
     * @function Decrypt
     * @param {Object} params
     * @param {string} params.publicKey The public key of the PKP
     * @param {string} params.ciphertext The ciphertext to decrypt
     * @returns {Promise<string>} The decrypted plaintext
     */
    function Decrypt({
      publicKey,
      ciphertext,
    }: {
      publicKey: string;
      ciphertext: string;
    }): Promise<string>;
    /**
     * @name Lit.Actions.Encrypt
     * @function Encrypt
     * @param {Object} params
     * @param {string} params.publicKey The public key of the PKP
     * @param {string} params.message The message to encrypt
     * @returns {Promise<string>} The ciphertext
     */
    function Encrypt({
      publicKey,
      message,
    }: {
      publicKey: string;
      message: string;
    }): Promise<string>;
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

/**
 * The jsonwebtoken library for JWT encoding, decoding, and verification.
 * See https://github.com/auth0/node-jsonwebtoken for full documentation.
 */
declare const jwt: {
  decode: (token: string, options?: any) => any;
  verify: (
    token: string,
    secretOrPublicKey: string | Buffer,
    options?: any,
  ) => any;
  sign: (
    payload: string | object | Buffer,
    secretOrPrivateKey: string | Buffer,
    options?: any,
  ) => string;
};
