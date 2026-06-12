/**
 * Request-signing primitives for venue APIs. Runs inside the Lit Actions
 * runtime: no Node `crypto`, no `Buffer`, no WebCrypto assumptions — only
 * @noble/* (bundled) and hand-rolled base64.
 *
 *  - HMAC-SHA256 hex        → Binance system-generated keys
 *  - Ed25519 base64         → Binance self-generated Ed25519 keys (their recommendation)
 *  - ES256 JWT (P-256)      → Coinbase Advanced Trade CDP keys
 */

import { hmac } from '@noble/hashes/hmac';
import { sha256 } from '@noble/hashes/sha256';
import { bytesToHex, hexToBytes, utf8ToBytes } from '@noble/hashes/utils';
import { ed25519 } from '@noble/curves/ed25519';
import { p256 } from '@noble/curves/p256';

// ---------------------------------------------------------------------------
// base64 (no Buffer/atob dependence — runtime-portable)

const B64_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

export function b64encode(bytes: Uint8Array): string {
  let out = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i]!;
    const b1 = bytes[i + 1];
    const b2 = bytes[i + 2];
    const n = (b0 << 16) | ((b1 ?? 0) << 8) | (b2 ?? 0);
    out +=
      B64_ALPHABET[(n >> 18) & 63]! +
      B64_ALPHABET[(n >> 12) & 63]! +
      (b1 === undefined ? '=' : B64_ALPHABET[(n >> 6) & 63]!) +
      (b2 === undefined ? '=' : B64_ALPHABET[n & 63]!);
  }
  return out;
}

export function b64decode(s: string): Uint8Array {
  const clean = s.replace(/[\s=]/g, '').replace(/-/g, '+').replace(/_/g, '/');
  const out = new Uint8Array(Math.floor((clean.length * 3) / 4));
  let acc = 0;
  let bits = 0;
  let idx = 0;
  for (const ch of clean) {
    const v = B64_ALPHABET.indexOf(ch);
    if (v < 0) throw new Error('invalid base64');
    acc = (acc << 6) | v;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out[idx++] = (acc >> bits) & 0xff;
    }
  }
  return out.subarray(0, idx);
}

export function b64urlEncode(bytes: Uint8Array): string {
  return b64encode(bytes).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/g, '');
}

// ---------------------------------------------------------------------------
// key parsing

/**
 * CDP key downloads (JSON) carry the PEM with LITERAL backslash-n sequences;
 * pasting that into an env var is the most common way keys arrive. Normalize
 * before any PEM detection/parsing.
 */
function normalizePemInput(s: string): string {
  return s.replace(/\\n/g, '\n').trim();
}

function pemToDer(pem: string): Uint8Array {
  const body = pem.replace(/-----(BEGIN|END)[^-]*-----/g, '').replace(/\s+/g, '');
  return b64decode(body);
}

function findSeq(hay: Uint8Array, needle: readonly number[]): number {
  outer: for (let i = 0; i <= hay.length - needle.length; i++) {
    for (let j = 0; j < needle.length; j++) {
      if (hay[i + j] !== needle[j]) continue outer;
    }
    return i;
  }
  return -1;
}

// PKCS8 Ed25519: SEQ { ver 0, alg 1.3.101.112, OCTET STRING { OCTET STRING seed } }
const PKCS8_ED25519_PREFIX = [
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
] as const;

/** Accepts PKCS8 PEM (real or literal-\n newlines), raw 64-char hex, or base64. Returns the 32-byte seed. */
export function parseEd25519PrivateKey(input: string): Uint8Array {
  const s = normalizePemInput(input);
  if (s.includes('-----BEGIN')) {
    const der = pemToDer(s);
    if (der.length === 48 && findSeq(der, PKCS8_ED25519_PREFIX) === 0) return der.subarray(16);
    if (der.length === 32) return der;
    throw new Error('lit-venues: unsupported Ed25519 PEM structure');
  }
  if (/^[0-9a-fA-F]{64}$/.test(s)) return hexToBytes(s.toLowerCase());
  const bytes = b64decode(s);
  if (bytes.length === 32) return bytes;
  if (bytes.length === 48 && findSeq(bytes, PKCS8_ED25519_PREFIX) === 0) return bytes.subarray(16);
  throw new Error('lit-venues: unsupported Ed25519 private key format');
}

// SEC1 ECPrivateKey (and PKCS8-wrapped SEC1) both contain: INTEGER 1, OCTET STRING(32) key
const EC_KEY_MARKER = [0x02, 0x01, 0x01, 0x04, 0x20] as const;

/** Accepts SEC1 or PKCS8 EC P-256 PEM (real or literal-\n newlines), raw 64-char hex, or base64. Returns the 32-byte scalar. */
export function parseEcP256PrivateKey(input: string): Uint8Array {
  const s = normalizePemInput(input);
  if (s.includes('-----BEGIN')) {
    const der = pemToDer(s);
    const at = findSeq(der, EC_KEY_MARKER);
    if (at >= 0 && der.length >= at + EC_KEY_MARKER.length + 32) {
      return der.subarray(at + EC_KEY_MARKER.length, at + EC_KEY_MARKER.length + 32);
    }
    throw new Error('lit-venues: unsupported EC PEM structure (expected SEC1 or PKCS8 P-256)');
  }
  if (/^[0-9a-fA-F]{64}$/.test(s)) return hexToBytes(s.toLowerCase());
  const bytes = b64decode(s);
  if (bytes.length === 32) return bytes;
  throw new Error('lit-venues: unsupported EC private key format');
}

// ---------------------------------------------------------------------------
// signers

export function hmacSha256Hex(secret: string, payload: string): string {
  return bytesToHex(hmac(sha256, utf8ToBytes(secret), utf8ToBytes(payload)));
}

/**
 * SHA-256 hex of a UTF-8 string. Handy for computing the `requestHash` that
 * binds an email approval to an exact operation (plan D6): hash a canonical
 * description of the operation, pass it to requestEmailApproval, and pass the
 * identical hash to checkEmailApproval so the runtime binds them in-TEE.
 */
export function sha256Hex(payload: string): string {
  return bytesToHex(sha256(utf8ToBytes(payload)));
}

export function ed25519SignBase64(privateKey: string, payload: string): string {
  const seed = parseEd25519PrivateKey(privateKey);
  return b64encode(ed25519.sign(utf8ToBytes(payload), seed));
}

let nonceCounter = 0;

/**
 * Hex of `byteLen` random bytes. Used only for uniqueness (JWT nonce, client
 * order ids) — never key material. Falls back to a hash of time+counter when
 * the runtime lacks crypto.getRandomValues.
 */
export function randomHex(byteLen: number): string {
  const g = (globalThis as { crypto?: { getRandomValues?: (b: Uint8Array) => Uint8Array } }).crypto;
  if (g?.getRandomValues) {
    const bytes = new Uint8Array(byteLen);
    g.getRandomValues(bytes);
    return bytesToHex(bytes);
  }
  const seed = `${Date.now()}-${++nonceCounter}-${Math.random()}`;
  return bytesToHex(sha256(utf8ToBytes(seed))).slice(0, byteLen * 2);
}

export interface Es256JwtInput {
  /** CDP API key name — becomes both `kid` and `sub`. */
  keyName: string;
  /** EC P-256 private key (SEC1/PKCS8 PEM, hex, or base64). */
  privateKey: string;
  /** Coinbase `uri` claim: "METHOD host/path" with NO query string. */
  uri: string;
  nowMs?: number;
  /** Coinbase caps JWT life at 2 minutes. */
  ttlSec?: number;
  /** Override for deterministic tests. */
  nonce?: string;
}

export function es256Jwt(input: Es256JwtInput): string {
  const nowSec = Math.floor((input.nowMs ?? Date.now()) / 1000);
  const header = {
    alg: 'ES256',
    kid: input.keyName,
    nonce: input.nonce ?? randomHex(16),
    typ: 'JWT',
  };
  const payload = {
    iss: 'cdp',
    nbf: nowSec,
    exp: nowSec + (input.ttlSec ?? 120),
    sub: input.keyName,
    uri: input.uri,
  };
  const signingInput =
    b64urlEncode(utf8ToBytes(JSON.stringify(header))) +
    '.' +
    b64urlEncode(utf8ToBytes(JSON.stringify(payload)));
  const priv = parseEcP256PrivateKey(input.privateKey);
  const sig = p256.sign(sha256(utf8ToBytes(signingInput)), priv);
  return `${signingInput}.${b64urlEncode(sig.toCompactRawBytes())}`;
}
