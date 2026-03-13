export declare namespace Lit {
  export namespace Actions {
    /**
     * Set the response returned to the client
     * @name Lit.Actions.setResponse
     * @function setResponse
     * @param {Object} params
     * @param {*} params.response The response to send to the client. If this is not a string, it will be JSON-encoded before being sent. A value of undefined is encoded as null.
     */
    function setResponse({ response }: { response: any }): any;
    /**
     * Decrypt data using AES with a symmetric key
     * @name Lit.Actions.Decrypt
     * @function Decrypt
     * @param {Object} params
     * @param {string} params.pkpId The ID of the PKP
     * @param {string} params.ciphertext The ciphertext to decrypt
     * @returns {Promise<string>} The decrypted plaintext
     */
    function Decrypt({
      pkpId,
      ciphertext,
    }: {
      pkpId: string;
      ciphertext: string;
    }): Promise<string>;
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
    }: {
      pkpId: string;
      message: string;
    }): Promise<string>;
    /**
     * Get the private key for a PKP wallet
     * @name Lit.Actions.getPrivateKey
     * @function getPrivateKey
     * @param {Object} params
     * @param {string} params.pkpId The ID of the PKP
     * @returns {Promise<string>} The private key secret
     */
    function getPrivateKey({ pkpId }: { pkpId: string }): Promise<string>;
    /**
     * Get the private key for the currently executing Lit Action
     * @name Lit.Actions.getLitActionPrivateKey
     * @function getLitActionPrivateKey
     * @returns {Promise<string>} The private key secret
     */
    function getLitActionPrivateKey(): Promise<string>;
    /**
     * Get the public key for a Lit Action by IPFS ID
     * @name Lit.Actions.getLitActionPublicKey
     * @function getLitActionPublicKey
     * @param {Object} params
     * @param {string} params.ipfsId The IPFS ID of the Lit Action
     * @returns {Promise<string>} The public key
     */
    function getLitActionPublicKey({
      ipfsId,
    }: {
      ipfsId: string;
    }): Promise<string>;
    /**
     * Get the wallet address for a Lit Action by IPFS ID
     * @name Lit.Actions.getLitActionWalletAddress
     * @function getLitActionWalletAddress
     * @param {Object} params
     * @param {string} params.ipfsId The IPFS ID of the Lit Action
     * @returns {Promise<string>} The wallet address
     */
    function getLitActionWalletAddress({
      ipfsId,
    }: {
      ipfsId: string;
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
