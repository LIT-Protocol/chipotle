/**
 * Minimal EIP-712 typed-data hashing + secp256k1 signing for PKP-native
 * venues (plan D8). Same constraints as signing.ts: @noble/* only, no Node
 * built-ins, runs inside the Lit Actions runtime.
 *
 * Only the field types Hyperliquid's schemas use are implemented; adding a
 * type is a one-line case below, but an unknown type throws rather than
 * guessing an encoding.
 */

import { keccak_256 } from '@noble/hashes/sha3';
import { secp256k1 } from '@noble/curves/secp256k1';
import { bytesToHex, concatBytes, hexToBytes, utf8ToBytes } from '@noble/hashes/utils';

export const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000';

/** Signature in the r/s/v wire shape venues expect. r and s are 0x-hex, minimally encoded (no zero padding) — matches eth_account's to_hex. */
export interface RsvSignature {
  r: string;
  s: string;
  v: number;
}

/** Anything that can produce an Ethereum signature over a 32-byte digest: a raw key (Node tests), the action-bound TEE key, or a custom MPC hook. */
export type SignFn = (digest: Uint8Array) => RsvSignature | Promise<RsvSignature>;

export type Eip712FieldType = 'string' | 'address' | 'uint64' | 'uint256' | 'bytes32' | 'bool';

export interface Eip712Field {
  name: string;
  type: Eip712FieldType;
}

export interface Eip712Domain {
  name: string;
  version: string;
  chainId: number;
  verifyingContract: string;
}

function strip0x(s: string): string {
  return s.startsWith('0x') || s.startsWith('0X') ? s.slice(2) : s;
}

function addressBytes(addr: string): Uint8Array {
  const hex = strip0x(addr).toLowerCase();
  if (!/^[0-9a-f]{40}$/.test(hex)) throw new Error(`invalid address: "${addr}"`);
  return hexToBytes(hex);
}

function word(fill: Uint8Array, rightAlign = true): Uint8Array {
  const out = new Uint8Array(32);
  out.set(fill, rightAlign ? 32 - fill.length : 0);
  return out;
}

function uintWord(value: number | string | bigint): Uint8Array {
  const v = BigInt(value);
  if (v < 0n) throw new Error('eip712: negative uint');
  const out = new Uint8Array(32);
  let rest = v;
  for (let i = 31; i >= 0 && rest > 0n; i--) {
    out[i] = Number(rest & 0xffn);
    rest >>= 8n;
  }
  if (rest > 0n) throw new Error('eip712: uint overflows 256 bits');
  return out;
}

function encodeValue(type: Eip712FieldType, value: unknown): Uint8Array {
  switch (type) {
    case 'string':
      return keccak_256(utf8ToBytes(String(value)));
    case 'address':
      return word(addressBytes(String(value)));
    case 'uint64':
    case 'uint256':
      return uintWord(value as number | string | bigint);
    case 'bytes32': {
      const bytes = hexToBytes(strip0x(String(value)));
      if (bytes.length !== 32) throw new Error('eip712: bytes32 must be 32 bytes');
      return bytes;
    }
    case 'bool':
      return word(Uint8Array.from([value ? 1 : 0]));
    default:
      throw new Error(`eip712: unsupported field type ${String(type)}`);
  }
}

export function hashStruct(typeName: string, fields: Eip712Field[], message: Record<string, unknown>): Uint8Array {
  const typeString = `${typeName}(${fields.map((f) => `${f.type} ${f.name}`).join(',')})`;
  const parts: Uint8Array[] = [keccak_256(utf8ToBytes(typeString))];
  for (const f of fields) {
    if (!(f.name in message)) throw new Error(`eip712: message missing field "${f.name}"`);
    parts.push(encodeValue(f.type, message[f.name]));
  }
  return keccak_256(concatBytes(...parts));
}

const DOMAIN_FIELDS: Eip712Field[] = [
  { name: 'name', type: 'string' },
  { name: 'version', type: 'string' },
  { name: 'chainId', type: 'uint256' },
  { name: 'verifyingContract', type: 'address' },
];

/** keccak256(0x1901 ‖ domainSeparator ‖ hashStruct(message)) — the digest an Ethereum wallet signs for typed data. */
export function typedDataDigest(
  domain: Eip712Domain,
  primaryType: string,
  fields: Eip712Field[],
  message: Record<string, unknown>,
): Uint8Array {
  const domainHash = hashStruct('EIP712Domain', DOMAIN_FIELDS, domain as unknown as Record<string, unknown>);
  const structHash = hashStruct(primaryType, fields, message);
  return keccak_256(concatBytes(Uint8Array.from([0x19, 0x01]), domainHash, structHash));
}

export function parseSecp256k1PrivateKey(input: string): Uint8Array {
  const hex = strip0x(input.trim()).toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(hex)) {
    throw new Error('lit-venues: secp256k1 private key must be 32 bytes of hex');
  }
  return hexToBytes(hex);
}

/** Ethereum address (0x-lowercase) for a secp256k1 private key. */
export function privateKeyToAddress(privateKey: string): string {
  const pub = secp256k1.getPublicKey(parseSecp256k1PrivateKey(privateKey), false);
  return `0x${bytesToHex(keccak_256(pub.subarray(1)).subarray(12))}`;
}

/** SignFn over a raw private key. In a Lit Action the key is the action-bound TEE key (`Lit.Actions.getLitActionPrivateKey()`); in Node tests it's a fixture. */
export function rawKeySigner(privateKey: string): SignFn {
  const priv = parseSecp256k1PrivateKey(privateKey);
  return (digest: Uint8Array): RsvSignature => {
    const sig = secp256k1.sign(digest, priv); // lowS, with recovery — Ethereum-compatible
    return { r: `0x${sig.r.toString(16)}`, s: `0x${sig.s.toString(16)}`, v: sig.recovery + 27 };
  };
}
