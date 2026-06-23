// lit-action-gating.js
//
// Gating / access control. Every gate FAILS CLOSED by throwing GateError; the
// importing action catches and translates into { authorized: false, reason }.
// Thresholds are arguments the action HARDCODES (trust-boundary rule in README).

import { addr, eip191Signer } from "micro-eth-signer@0.19.0";
import { deny } from "./lit-action-core.js";
import { rpc } from "./lit-action-rpc.js";
import { ethCallContract, ERC20, ERC1155, SANCTIONS } from "./lit-action-onchain.js";

// Chainalysis on-chain sanctions oracle (Ethereum mainnet). Hardcoded here so
// the screened set is fixed by the action's CID, not by the caller.
export const CHAINALYSIS_SANCTIONS_ORACLE =
  "0x40C57923924B5c5c5455c48D93317139ADDaC8fb";

/** Deny unless native balance >= minWei. */
export async function gateEthBalance({ address, minWei, rpcUrl }) {
  const bal = BigInt(await rpc(rpcUrl, "eth_getBalance", [address, "latest"]));
  if (bal < BigInt(minWei)) deny(`balance ${bal} < required ${minWei}`);
  return bal;
}

/** Deny unless ERC-20 balanceOf(holder) >= minAmount. */
export async function gateErc20Balance({ holder, token, minAmount, rpcUrl }) {
  const bal = BigInt(await ethCallContract(rpcUrl, token, ERC20, "balanceOf", { o: holder }));
  if (bal < BigInt(minAmount)) deny(`token balance ${bal} < required ${minAmount}`);
  return bal;
}

/**
 * Deny unless the holder owns the NFT. ERC-721: balanceOf(holder) > 0. ERC-1155:
 * pass `tokenId` and we check balanceOf(holder, tokenId) > 0.
 */
export async function gateNftOwnership({ holder, nftContract, rpcUrl, tokenId }) {
  const bal =
    tokenId === undefined
      ? BigInt(await ethCallContract(rpcUrl, nftContract, ERC20, "balanceOf", { o: holder }))
      : BigInt(await ethCallContract(rpcUrl, nftContract, ERC1155, "balanceOf", { o: holder, id: BigInt(tokenId) }));
  if (bal <= 0n) deny(`holder ${holder} owns none of ${nftContract}`);
  return bal;
}

/** Reject outside the inclusive [startUnix, endUnix] window. */
export function gateTimeWindow({ startUnix, endUnix }) {
  const now = Math.floor(Date.now() / 1000);
  if (now < startUnix) deny(`window not open yet (${startUnix - now}s to go)`);
  if (now > endUnix) deny(`window closed (${now - endUnix}s ago)`);
  return now;
}

/**
 * Staticcall the Chainalysis sanctions oracle on a hostname-pinned mainnet RPC
 * and deny if the address is sanctioned. `screeningRpcUrl` must already be
 * pinned by the caller (see assertAllowedRpc).
 */
export async function gateSanctions({ address, screeningRpcUrl }) {
  const sanctioned = await ethCallContract(
    screeningRpcUrl,
    CHAINALYSIS_SANCTIONS_ORACLE,
    SANCTIONS,
    "isSanctioned",
    { a: address },
  );
  if (sanctioned) deny(`address ${address} is on the sanctions list`);
  return false;
}

/** POST to an auth service and require { valid: true }. */
export async function gateAuthToken({ token, verifyUrl }) {
  const res = await fetch(verifyUrl, {
    method: "POST",
    redirect: "error",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ token }),
  });
  if (!res.ok) deny(`auth service -> HTTP ${res.status}`);
  const body = await res.json();
  if (body.valid !== true) deny("auth token rejected");
  return true;
}

/**
 * Recover the signer of an EIP-191 message and require it to equal
 * `allowedAddress`. The message MUST carry a nonce/deadline for replay
 * protection (the caller builds that into the message text). Returns the
 * recovered address.
 */
export function requireCallerSignature({ message, signature, allowedAddress }) {
  let recovered;
  try {
    recovered = eip191Signer.recoverPublicKey(signature, message);
  } catch (e) {
    deny(`signature did not recover: ${e.message}`);
  }
  if (recovered.toLowerCase() !== allowedAddress.toLowerCase()) {
    deny(`unauthorized: signer ${recovered} != allowed ${allowedAddress}`);
  }
  return recovered;
}

/**
 * Case-insensitive allowlist check for a PKP address. Relies on the gateway
 * having already proven PKP ownership. `allowedPkp` may be a string or array.
 */
export function requirePkpAllowed({ pkpAddress, allowedPkp }) {
  const allow = (Array.isArray(allowedPkp) ? allowedPkp : [allowedPkp]).map((a) =>
    a.toLowerCase(),
  );
  if (!allow.includes(pkpAddress.toLowerCase())) {
    deny(`pkp ${pkpAddress} not in allowlist`);
  }
  return true;
}

/**
 * Run several gate thunks in order and fail closed on the first denial. Since
 * gating is just code, AND/OR composition is free -- compose your own OR by
 * try/catching individual gates. Returns the array of gate results.
 */
export async function combineGates(gateFns) {
  const results = [];
  for (const fn of gateFns) {
    results.push(await fn());
  }
  return results;
}
