/**
 * Global reference to the Lit Actions namespace for convenience.
 * This alias is injected in the Lit Action execution environment and mirrors `Lit.Actions`.
 *
 * @global
 * @name LitActions
 * @type {typeof import('./02_litActionsSDK.js')}
 */
const LitActions = undefined;

/**
 * The ethers.js v5 API exposed to Lit Actions for interacting with EVM chains.
 * Includes wallets, providers, contracts, and cryptographic helpers.
 *
 * @global
 * @name ethers
 * @type {typeof import('ethers')}
 */
const ethers = undefined;

export {};
