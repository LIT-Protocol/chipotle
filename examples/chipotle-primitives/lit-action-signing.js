// lit-action-signing.js
//
// Signing & action identity. EIP-191 personal-sign with either a PKP wallet key
// or the action's CID-derived key, plus the authorization-digest builder that
// every authorization primitive is built on. Uses micro-eth-signer's
// eip191Signer (signatures are 0x{r}{s}{v}, v in 27/28 -- ecrecover-compatible).

import { addr, eip191Signer } from "micro-eth-signer@0.19.0";
import { keccak256, abiEncode, arrayify } from "./lit-action-core.js";

/**
 * EIP-191 personal-sign of `message` (string or bytes) with a PKP wallet key.
 * Returns { signature, signer }.
 */
export async function signWithPkp({ pkpId, message }) {
  const key = await Lit.Actions.getPrivateKey({ pkpId });
  return { signature: eip191Signer.sign(message, key), signer: addr.fromPrivateKey(key) };
}

/**
 * EIP-191 personal-sign of `message` with the action's CID-derived key
 * (immutable-proof identity). Returns { signature, signer }.
 */
export async function signWithAction({ message }) {
  const key = await Lit.Actions.getLitActionPrivateKey();
  return { signature: eip191Signer.sign(message, key), signer: addr.fromPrivateKey(key) };
}

/**
 * Build the ready-to-sign authorization digest: abi.encode(types, values) ->
 * keccak256 -> arrayify. Returns { digest (0x hex), digestBytes (Uint8Array) }.
 * This is the exact tuple your verifying contract re-derives before ecrecover.
 */
export function buildAuthDigest({ types, values }) {
  const digest = keccak256(abiEncode(types, values));
  return { digest, digestBytes: arrayify(digest) };
}

/**
 * buildAuthDigest + EIP-191 sign in one call -- the core of every authorization
 * primitive. Signs with the action key (useAction, default) or a PKP key.
 * Returns { signature, signer, digest }.
 */
export async function signDigest({ types, values, useAction = true, pkpId }) {
  const { digest, digestBytes } = buildAuthDigest({ types, values });
  const key = useAction
    ? await Lit.Actions.getLitActionPrivateKey()
    : await Lit.Actions.getPrivateKey({ pkpId });
  return {
    signature: eip191Signer.sign(digestBytes, key),
    signer: addr.fromPrivateKey(key),
    digest,
  };
}

/** Recover the EIP-191 signer address of (message, signature). */
export function recoverSigner({ message, signature }) {
  return eip191Signer.recoverPublicKey(signature, message);
}

/** Derive a sibling action's wallet address from its IPFS CID. */
export async function getActionAddress({ ipfsId }) {
  return Lit.Actions.getLitActionWalletAddress({ ipfsId });
}
