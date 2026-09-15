// lit-action-rpc.js
//
// RPC trust anchors & chain safety. A caller can pass any rpcUrl in js_params,
// so the host must be pinned against the action's HARDCODED policy (see the
// trust-boundary rule in README.md). `policy` is the action's constant map,
// never js_params.

import { deny } from "./lit-action-core.js";

/**
 * Hardened JSON-RPC POST. `redirect: "error"` closes the open-redirect-after-pin
 * hole (a pinned host that 30x-redirects to an attacker). Throws on HTTP error
 * and on JSON-RPC error.
 */
export async function rpc(url, method, params = []) {
  const res = await fetch(url, {
    method: "POST",
    redirect: "error",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  if (!res.ok) throw new Error(`rpc ${method} -> HTTP ${res.status}`);
  const body = await res.json();
  if (body.error) throw new Error(`rpc ${method} -> ${body.error.message}`);
  return body.result;
}

/**
 * Require https + a hostname that matches the per-chain policy, and return that
 * per-chain policy (e.g. { hostRegex, minConfirmations }). `policy` must be the
 * action's HARDCODED map, never js_params.
 */
export function assertAllowedRpc({ chainId, rpcUrl, policy }) {
  const chainPolicy = policy && policy[chainId];
  if (!chainPolicy) deny(`no RPC policy for chainId ${chainId}`);
  let u;
  try {
    u = new URL(rpcUrl);
  } catch {
    deny("rpcUrl is not a valid URL");
  }
  if (u.protocol !== "https:") deny(`rpcUrl must be https, got ${u.protocol}`);
  if (!chainPolicy.hostRegex.test(u.hostname)) {
    deny(`rpc host not whitelisted: ${u.hostname}`);
  }
  return chainPolicy;
}

/** Cross-check eth_chainId. Only meaningful AFTER the hostname pin. */
export async function assertChainId({ rpcUrl, expectedChainId }) {
  const got = Number(await rpc(rpcUrl, "eth_chainId", []));
  if (got !== Number(expectedChainId)) {
    deny(`rpc chainId ${got} != expected ${expectedChainId}`);
  }
  return got;
}

/** Reorg defense: don't sign until the source block is buried `minConf` deep. */
export async function requireConfirmations({ rpcUrl, blockNumber, minConf }) {
  const head = Number(await rpc(rpcUrl, "eth_blockNumber", []));
  const confirmations = head - Number(blockNumber) + 1;
  if (confirmations < minConf) {
    deny(`only ${confirmations} confirmations, need ${minConf}`);
  }
  return confirmations;
}
