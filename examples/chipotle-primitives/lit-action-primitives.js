// lit-action-primitives.js
//
// Barrel module: re-exports every lit-action-* primitive so an action can pull
// the whole catalog with one import.
//
//   import * as P from "./lit-action-primitives.js";
//   P.assertAllowedRpc(...); P.signDigest(...);
//
// Or import a single concern directly, e.g.
//   import { gateEthBalance, combineGates } from "./lit-action-gating.js";
//
// NOTE: these modules use relative imports between each other, which the Lit
// Action runtime does not resolve on its own -- bundle them (or vendor the set)
// before deploy. See README.md > "Consuming this module from an action".

export * from "./lit-action-core.js";     // keccak256, abiEncode, arrayify, GateError, deny
export * from "./lit-action-rpc.js";      // rpc, assertAllowedRpc, assertChainId, requireConfirmations
export * from "./lit-action-onchain.js";  // readContract, readAndValidateLog, sendEth, bindToOnchainOrder
export * from "./lit-action-gating.js";   // gate*, requireCallerSignature, requirePkpAllowed, combineGates
export * from "./lit-action-signing.js";  // signWithPkp, signWithAction, buildAuthDigest, signDigest, recoverSigner, getActionAddress
export * from "./lit-action-secrets.js";  // sealToVault, openFromVault, withSecret, assembleSecretRpcUrl
export * from "./lit-action-oracles.js";  // requireStrictAgreement, medianWithSpreadCheck, signedPriceFeed, aiConsensus
export * from "./lit-action-replay.js";   // newNonce, withDeadline, assertNotExpired
