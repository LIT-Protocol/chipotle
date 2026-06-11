/**
 * Minimal msgpack encoder — exactly the subset Hyperliquid's action hash needs
 * (maps in insertion order, arrays, strings, bools, ints, nil). The byte
 * output must match `msgpack.packb` in the official Python SDK, which is why:
 *
 *  - map keys are emitted in object insertion order (NEVER sorted),
 *  - non-integer numbers throw — px/sz travel as decimal strings, and a float
 *    that silently packed as float64 would change the action hash,
 *  - `undefined` values are skipped (mirrors JSON.stringify for the POSTed
 *    body, and Python never has the key at all).
 *
 * Decoding is deliberately not implemented; nothing in-TEE consumes msgpack.
 */

import { utf8ToBytes } from '@noble/hashes/utils';

function pushUintBE(out: number[], v: bigint, bytes: number): void {
  for (let i = bytes - 1; i >= 0; i--) {
    out.push(Number((v >> BigInt(8 * i)) & 0xffn));
  }
}

function encodeInt(out: number[], v: bigint): void {
  if (v >= 0n) {
    if (v < 0x80n) out.push(Number(v));
    else if (v <= 0xffn) {
      out.push(0xcc, Number(v));
    } else if (v <= 0xffffn) {
      out.push(0xcd);
      pushUintBE(out, v, 2);
    } else if (v <= 0xffffffffn) {
      out.push(0xce);
      pushUintBE(out, v, 4);
    } else if (v <= 0xffffffffffffffffn) {
      out.push(0xcf);
      pushUintBE(out, v, 8);
    } else {
      throw new Error('lit-venues msgpack: integer exceeds uint64');
    }
  } else {
    if (v >= -32n) out.push(0xe0 | Number(v & 0x1fn));
    else if (v >= -0x80n) {
      out.push(0xd0, Number(v & 0xffn));
    } else if (v >= -0x8000n) {
      out.push(0xd1);
      pushUintBE(out, v & 0xffffn, 2);
    } else if (v >= -0x80000000n) {
      out.push(0xd2);
      pushUintBE(out, v & 0xffffffffn, 4);
    } else if (v >= -0x8000000000000000n) {
      out.push(0xd3);
      pushUintBE(out, v & 0xffffffffffffffffn, 8);
    } else {
      throw new Error('lit-venues msgpack: integer exceeds int64');
    }
  }
}

function encode(out: number[], v: unknown): void {
  if (v === null || v === undefined) {
    out.push(0xc0);
  } else if (typeof v === 'boolean') {
    out.push(v ? 0xc3 : 0xc2);
  } else if (typeof v === 'number') {
    if (!Number.isInteger(v)) {
      throw new Error(
        'lit-venues msgpack: non-integer numbers are forbidden in actions — pass decimal strings (float drift would change the action hash)',
      );
    }
    encodeInt(out, BigInt(v));
  } else if (typeof v === 'bigint') {
    encodeInt(out, v);
  } else if (typeof v === 'string') {
    const bytes = utf8ToBytes(v);
    if (bytes.length <= 31) out.push(0xa0 | bytes.length);
    else if (bytes.length <= 0xff) out.push(0xd9, bytes.length);
    else if (bytes.length <= 0xffff) {
      out.push(0xda);
      pushUintBE(out, BigInt(bytes.length), 2);
    } else {
      throw new Error('lit-venues msgpack: string too long');
    }
    for (const b of bytes) out.push(b);
  } else if (Array.isArray(v)) {
    if (v.length <= 15) out.push(0x90 | v.length);
    else if (v.length <= 0xffff) {
      out.push(0xdc);
      pushUintBE(out, BigInt(v.length), 2);
    } else {
      throw new Error('lit-venues msgpack: array too long');
    }
    for (const item of v) encode(out, item);
  } else if (typeof v === 'object') {
    const entries = Object.entries(v as Record<string, unknown>).filter(([, val]) => val !== undefined);
    if (entries.length <= 15) out.push(0x80 | entries.length);
    else if (entries.length <= 0xffff) {
      out.push(0xde);
      pushUintBE(out, BigInt(entries.length), 2);
    } else {
      throw new Error('lit-venues msgpack: map too large');
    }
    for (const [k, val] of entries) {
      encode(out, k);
      encode(out, val);
    }
  } else {
    throw new Error(`lit-venues msgpack: unsupported type ${typeof v}`);
  }
}

export function msgpackEncode(value: unknown): Uint8Array {
  const out: number[] = [];
  encode(out, value);
  return Uint8Array.from(out);
}
