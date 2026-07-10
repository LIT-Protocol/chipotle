// lit-action-core.js
//
// Shared low-level helpers used across the other lit-action-* modules: keccak,
// ABI tuple coding, hex<->bytes, and the GateError / deny() denial convention.
//
// We deliberately do not hand-roll crypto or ABI encoding. Everything here is
// glue around audited libraries from Paul Miller (paulmillr): micro-eth-signer
// (ABI coder, wei math, hex coder) and @noble/hashes (keccak-256, byte utils).
//
// See README.md for the trust-boundary rule and for how the bare, version-pinned
// jsDelivr import specifiers below are resolved by the Lit Action runtime.

import { ethHex } from "micro-eth-signer@0.19.0";
import { createContract } from "micro-eth-signer@0.19.0/advanced/abi.js";
import { keccak_256 } from "@noble/hashes@1.4.0/sha3/+esm";
import {
  bytesToHex,
  hexToBytes,
  utf8ToBytes,
} from "@noble/hashes@1.4.0/utils/+esm";

// Re-export the audited byte utils so sibling modules import them from one place.
export { bytesToHex, hexToBytes, utf8ToBytes };

// Denials are thrown, never returned. Gates fail closed: the first thrown
// GateError aborts the action, and the caller's `main` is expected to catch and
// translate it into `{ authorized: false, reason }`.
export class GateError extends Error {
  constructor(reason) {
    super(reason);
    this.name = "GateError";
  }
}

export const deny = (reason) => {
  throw new GateError(reason);
};

/** keccak-256 of raw bytes, returned as a 0x-prefixed hex string. */
export function keccak256(bytes) {
  return "0x" + bytesToHex(keccak_256(bytes));
}

/** keccak-256 of a UTF-8 string (e.g. an event signature or question text). */
export function keccak256Utf8(text) {
  return keccak256(utf8ToBytes(text));
}

/** 0x-hex string -> Uint8Array (ethers `arrayify` equivalent). */
export function arrayify(hex) {
  return ethHex.decode(hex);
}

// Coerce a single JS value to what micro-eth-signer's ABI coder (micro-packed)
// expects for `type`: bigint for integers, 0x-hex string for address, raw bytes
// for bytesN/bytes, boolean for bool, string otherwise.
function coerceAbiValue(type, v) {
  if (/^u?int\d*$/.test(type)) return BigInt(v);
  if (type === "address") return typeof v === "string" ? v : ethHex.encode(v);
  if (/^bytes\d*$/.test(type)) {
    return typeof v === "string" ? hexToBytes(v.replace(/^0x/, "")) : v;
  }
  if (type === "bool") return Boolean(v);
  return v;
}

/**
 * ABI-encode an arbitrary tuple of solidity types + values, matching ethers'
 * `defaultAbiCoder.encode(types, values)`. Implemented by building a synthetic
 * function fragment, encoding through micro-eth-signer's audited ABI coder, and
 * stripping the 4-byte selector -- so the bytes are the bare head/tail tuple
 * encoding a Solidity contract re-derives with `abi.encode`.
 *
 * @param {string[]} types e.g. ["address","uint256","bytes32"]
 * @param {any[]} values   integers as number|bigint|string, address/bytes as 0x-hex
 * @returns {Uint8Array}
 */
export function abiEncode(types, values) {
  if (types.length !== values.length) {
    throw new Error("abiEncode: types/values length mismatch");
  }
  const inputs = types.map((type, i) => ({ name: `a${i}`, type }));
  const c = createContract([
    { type: "function", name: "f", inputs, outputs: [] },
  ]);
  const arg = {};
  inputs.forEach((inp, i) => {
    arg[inp.name] = coerceAbiValue(inp.type, values[i]);
  });
  // encodeInput returns selector(4) || abi.encode(tuple); drop the selector.
  return c.f.encodeInput(arg).slice(4);
}

/**
 * First value out of a decodeOutput result (single-return coders give the bare
 * value; multi-return give a named object).
 */
export function firstValue(decoded) {
  if (decoded && typeof decoded === "object" && !Array.isArray(decoded)) {
    const vals = Object.values(decoded);
    return vals.length === 1 ? vals[0] : decoded;
  }
  return decoded;
}
