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

export {
  keccak256,
  keccak256Utf8,
  arrayify,
  abiEncode,
  GateError,
  deny,
} from "./lit-action-core.js";
export {
  rpc,
  assertAllowedRpc,
  assertChainId,
  requireConfirmations,
} from "./lit-action-rpc.js";
export {
  readContract,
  readAndValidateLog,
  sendEth,
  bindToOnchainOrder,
} from "./lit-action-onchain.js";
export {
  gateEthBalance,
  gateErc20Balance,
  gateNftOwnership,
  gateTimeWindow,
  gateSanctions,
  gateAuthToken,
  requireCallerSignature,
  requirePkpAllowed,
  combineGates,
} from "./lit-action-gating.js";
export {
  signWithPkp,
  signWithAction,
  buildAuthDigest,
  signDigest,
  recoverSigner,
  getActionAddress,
} from "./lit-action-signing.js";
export {
  sealToVault,
  openFromVault,
  withSecret,
  assembleSecretRpcUrl,
} from "./lit-action-secrets.js";
export {
  requireStrictAgreement,
  medianWithSpreadCheck,
  signedPriceFeed,
  aiConsensus,
} from "./lit-action-oracles.js";
export {
  newNonce,
  withDeadline,
  assertNotExpired,
} from "./lit-action-replay.js";
