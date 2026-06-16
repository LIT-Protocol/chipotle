/**
 * Crypto primitives for the approval primitive. Runs inside the Lit Actions
 * runtime: no Node `crypto`, no `Buffer` — only @noble/* (bundled).
 *
 * One CID-bound TEE key (the approval action's `getLitActionPrivateKey()`)
 * powers two things, domain-separated:
 *   - the OTP HMAC key (so a store adversary can't brute-force the 6-digit OTP)
 *   - the secp256k1 attestation signing key (its pubkey is what consuming
 *     actions pin to verify approvals)
 */

import { hmac } from '@noble/hashes/hmac';
import { sha256 } from '@noble/hashes/sha256';
import { bytesToHex, hexToBytes, utf8ToBytes } from '@noble/hashes/utils';
import { secp256k1 } from '@noble/curves/secp256k1';

export function sha256Hex(payload: string): string {
  return bytesToHex(sha256(utf8ToBytes(payload)));
}

function toKeyBytes(privateKey: string | Uint8Array): Uint8Array {
  if (privateKey instanceof Uint8Array) return privateKey;
  return hexToBytes(privateKey.trim().replace(/^0x/, '').toLowerCase());
}

/** Domain-separated subkey from the action's signing key. Keeps the OTP HMAC
 *  key distinct from the signing scalar even though both come from one CID key. */
export function deriveOtpKey(signingKey: string | Uint8Array): Uint8Array {
  return hmac(sha256, toKeyBytes(signingKey), utf8ToBytes('lit-approvals/otp-hmac/v1'));
}

export function otpHmacHex(otpKey: Uint8Array, approvalId: string, otp: string): string {
  return bytesToHex(hmac(sha256, otpKey, utf8ToBytes(`${approvalId}:${otp}`)));
}

/** Length-safe constant-time-ish hex comparison. */
export function timingSafeEqualHex(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

/** secp256k1 ECDSA over sha256(payloadBytes), compact 64-byte sig as hex.
 *  Matches the runtime verifier's `secp256k1-sha256`. */
export function signPayload(payload: string, signingKey: string | Uint8Array): string {
  const msgHash = sha256(utf8ToBytes(payload));
  const sig = secp256k1.sign(msgHash, toKeyBytes(signingKey));
  return bytesToHex(sig.toCompactRawBytes());
}

export function verifyPayloadSig(payload: string, sigHex: string, pubKeyHex: string): boolean {
  try {
    const msgHash = sha256(utf8ToBytes(payload));
    return secp256k1.verify(hexToBytes(sigHex), msgHash, hexToBytes(pubKeyHex.replace(/^0x/, '')));
  } catch {
    return false;
  }
}

/** SEC1-compressed pubkey hex for the signing key — pin this in consuming
 *  actions (the analog of the old LIT_APPROVAL_ATTESTATION_PUBKEY). */
export function publicKeyHex(signingKey: string | Uint8Array, compressed = true): string {
  return bytesToHex(secp256k1.getPublicKey(toKeyBytes(signingKey), compressed));
}

/** Randomness seam — inject `randomBytes` (default: crypto.getRandomValues).
 *  Throws rather than silently degrading to weak randomness for security
 *  material (OTP, approvalId). */
export type RandomBytes = (n: number) => Uint8Array;

export const defaultRandomBytes: RandomBytes = (n: number) => {
  const g = (globalThis as { crypto?: { getRandomValues?: (b: Uint8Array) => Uint8Array } }).crypto;
  if (!g?.getRandomValues) {
    throw new Error('lit-approvals: no CSPRNG (crypto.getRandomValues) — inject randomBytes');
  }
  const bytes = new Uint8Array(n);
  g.getRandomValues(bytes);
  return bytes;
};

export function genApprovalId(randomBytes: RandomBytes): string {
  return bytesToHex(randomBytes(16));
}

/** Uniform 6-digit OTP via rejection sampling (no modulo bias). */
export function genOtp(randomBytes: RandomBytes): string {
  for (;;) {
    const b = randomBytes(4);
    const n = ((b[0]! << 24) | (b[1]! << 16) | (b[2]! << 8) | b[3]!) >>> 0;
    if (n < 4_000_000_000) return String(n % 1_000_000).padStart(6, '0');
  }
}
