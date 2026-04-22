/**
 * Transaction lifecycle state machine + revert-reason decoder for
 * sovereign-mode direct contract writes.
 *
 * States (monotonic, with only `confirmed|failed|reorged` as terminal):
 *   preparing    → building calldata, validating mode/signer/chain
 *   previewing   → awaiting user's in-dashboard preview/confirm click
 *   signing      → wallet popup is open (user must confirm in wallet)
 *   pending      → tx broadcast to mempool; hash known; waiting for 1 conf
 *   confirming   → tx included in a block; waiting for target confirmations
 *   confirmed    → TERMINAL. receipt.status === 1, target confirmations met.
 *   failed       → TERMINAL. rejected, reverted, timed out, unparseable.
 *   reorged      → TERMINAL. receipt became null after earlier inclusion.
 *
 * Revert parsing mirrors `lit-api-server/src/accounts/decode_revert.rs`:
 *   1. Error(string) / Panic(uint256) (standard solidity reverts)
 *   2. AccountConfig custom errors (from ACCOUNT_CONFIG_ERROR_ABI)
 *   3. Unknown revert data (truncated hex)
 */

import { ACCOUNT_CONFIG_ERROR_ABI } from './account_config_full_abi.js';

export const TX_STATES = Object.freeze({
  PREPARING: 'preparing',
  PREVIEWING: 'previewing',
  SIGNING: 'signing',
  PENDING: 'pending',
  CONFIRMING: 'confirming',
  CONFIRMED: 'confirmed',
  FAILED: 'failed',
  REORGED: 'reorged',
});

const TERMINAL_STATES = new Set([TX_STATES.CONFIRMED, TX_STATES.FAILED, TX_STATES.REORGED]);

export function isTerminal(state) {
  return TERMINAL_STATES.has(state);
}

const MAX_HEX_DISPLAY_BYTES = 64;

function getEthers() {
  if (typeof globalThis.ethers !== 'undefined') return globalThis.ethers;
  throw new Error('tx_lifecycle: ethers is not loaded. Preload it before importing this module.');
}

/**
 * Decode an Error(string) / Panic(uint256) revert payload. Returns null if
 * the payload doesn't match those two standard selectors.
 */
function decodeStandardRevert(ethers, data) {
  if (!data || data === '0x') return null;
  const selector = data.slice(0, 10).toLowerCase();
  try {
    if (selector === '0x08c379a0') {
      const [reason] = ethers.AbiCoder.defaultAbiCoder().decode(['string'], '0x' + data.slice(10));
      return `Revert: ${reason}`;
    }
    if (selector === '0x4e487b71') {
      const [code] = ethers.AbiCoder.defaultAbiCoder().decode(['uint256'], '0x' + data.slice(10));
      return `Panic: 0x${code.toString(16).padStart(2, '0')}`;
    }
  } catch {
    return null;
  }
  return null;
}

/**
 * Decode a custom-error revert payload via the AccountConfig error ABI.
 * Returns null if the 4-byte selector doesn't match any known custom error.
 */
function decodeCustomError(ethers, data) {
  if (!data || data === '0x' || data.length < 10) return null;
  const iface = new ethers.Interface(ACCOUNT_CONFIG_ERROR_ABI);
  try {
    const parsed = iface.parseError(data);
    if (!parsed) return null;
    const argStr = parsed.args.map((a) => String(a)).join(', ');
    return `Contract error: ${parsed.name}(${argStr})`;
  } catch {
    return null;
  }
}

/**
 * Top-level decoder. Accepts an ethers error (or any object with a nested
 * `data` / `error.data` / `info.error.data` / `revert` / `cause` chain).
 * Always returns a non-empty string; falls back to the error's message or
 * `String(err)` if nothing else matches.
 */
export function decodeContractRevert(err) {
  const ethers = getEthers();
  const data = extractRevertData(err);

  if (data) {
    const std = decodeStandardRevert(ethers, data);
    if (std) return std;

    const custom = decodeCustomError(ethers, data);
    if (custom) return custom;

    if (data.length > 2 + MAX_HEX_DISPLAY_BYTES * 2) {
      return `Unknown revert data (${(data.length - 2) / 2} bytes): ${data.slice(0, 2 + MAX_HEX_DISPLAY_BYTES * 2)}...`;
    }
    return `Unknown revert data: ${data}`;
  }

  if (err && typeof err === 'object') {
    if (err.shortMessage) return err.shortMessage;
    if (err.reason) return err.reason;
    if (err.message) return err.message;
  }
  return String(err);
}

/**
 * Walk the ethers v6 error graph looking for a 0x-hex revert payload. Ethers
 * hides the payload in different slots depending on provider type; this
 * tries every known location.
 */
function extractRevertData(err) {
  if (!err) return null;
  const candidates = [
    err.data,
    err.error?.data,
    err.info?.error?.data,
    err.revert?.data,
    err.cause?.data,
    err.cause?.error?.data,
    err.cause?.info?.error?.data,
  ];
  for (const c of candidates) {
    if (typeof c === 'string' && c.startsWith('0x')) return c;
    if (c && typeof c === 'object' && typeof c.data === 'string' && c.data.startsWith('0x')) return c.data;
  }
  if (err.cause) return extractRevertData(err.cause);
  return null;
}

/**
 * Run a sovereign-mode contract write end-to-end with lifecycle callbacks.
 *
 * @param {Object} options
 * @param {import('ethers').Contract} options.contract - ethers v6 contract (already connected to a signer)
 * @param {string} options.method - function name on the contract
 * @param {any[]} options.args - positional args for the call
 * @param {Object} [options.overrides] - ethers tx overrides (gasLimit, value, etc.)
 * @param {number} [options.confirmations=1] - confirmations to wait for before marking confirmed
 * @param {(state: string, payload?: Object) => void} [options.onState] - called on every state transition
 * @param {() => Promise<boolean>} [options.onPreview] - user-preview gate; must resolve true to proceed
 * @returns {Promise<{receipt: Object, txHash: string}>}
 * @throws {Error} with { state: 'failed'|'reorged', cause, decoded } on failure
 */
export async function runContractWrite({
  contract,
  method,
  args,
  overrides = {},
  confirmations = 1,
  onState = () => {},
  onPreview,
}) {
  let state = TX_STATES.PREPARING;
  const emit = (next, payload) => {
    state = next;
    try {
      onState(next, payload);
    } catch (cbErr) {
      console.warn('[tx_lifecycle] onState callback threw:', cbErr);
    }
  };

  try {
    emit(TX_STATES.PREPARING, { method, args });

    if (typeof contract[method] !== 'function') {
      throw new Error(`tx_lifecycle: contract has no method "${method}"`);
    }

    if (onPreview) {
      emit(TX_STATES.PREVIEWING, { method, args });
      const ok = await onPreview(method, args);
      if (!ok) throw Object.assign(new Error('User cancelled at preview'), { userCancelled: true });
    }

    emit(TX_STATES.SIGNING, { method, args });
    const tx = await contract[method](...args, overrides);

    emit(TX_STATES.PENDING, { txHash: tx.hash, method });
    const receipt = await tx.wait(1);
    if (!receipt) {
      throw Object.assign(new Error('Transaction receipt was null (possible reorg)'), { reorg: true });
    }
    if (receipt.status !== 1) {
      throw Object.assign(new Error('Transaction reverted on-chain'), { receipt });
    }

    if (confirmations > 1) {
      emit(TX_STATES.CONFIRMING, { txHash: tx.hash, confirmations: 1, target: confirmations });
      await tx.wait(confirmations);
    }

    emit(TX_STATES.CONFIRMED, { txHash: tx.hash, receipt });
    return { receipt, txHash: tx.hash };
  } catch (err) {
    const decoded = decodeContractRevert(err);
    const finalState = err?.reorg ? TX_STATES.REORGED : TX_STATES.FAILED;
    emit(finalState, { error: err, decoded });
    const wrapped = new Error(decoded);
    wrapped.cause = err;
    wrapped.state = finalState;
    wrapped.decoded = decoded;
    wrapped.userCancelled = !!err?.userCancelled;
    throw wrapped;
  }
}

/**
 * Human-readable state label for UI surfaces.
 */
export function describeState(state) {
  switch (state) {
    case TX_STATES.PREPARING: return 'Preparing transaction…';
    case TX_STATES.PREVIEWING: return 'Awaiting your confirmation…';
    case TX_STATES.SIGNING: return 'Awaiting wallet signature…';
    case TX_STATES.PENDING: return 'Broadcast — waiting for inclusion…';
    case TX_STATES.CONFIRMING: return 'Included — waiting for more confirmations…';
    case TX_STATES.CONFIRMED: return 'Transaction confirmed.';
    case TX_STATES.FAILED: return 'Transaction failed.';
    case TX_STATES.REORGED: return 'Reorganized — resubmit may be required.';
    default: return state;
  }
}
