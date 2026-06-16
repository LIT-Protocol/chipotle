import { describe, it, expect } from 'vitest';
import { sha256Hex, genOtp, deriveOtpKey, otpHmacHex, publicKeyHex, signPayload, verifyPayloadSig } from '../src/crypto';
import { seededRandom } from './memstore';

describe('crypto primitives', () => {
  it('sha256Hex matches the known vector for "abc"', () => {
    expect(sha256Hex('abc')).toBe('ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
  });

  it('genOtp returns a uniform 6-digit string', () => {
    const rand = seededRandom(42);
    for (let i = 0; i < 100; i++) expect(genOtp(rand)).toMatch(/^\d{6}$/);
  });

  it('otpKey is domain-separated and deterministic for a given signing key', () => {
    const k1 = deriveOtpKey('11'.repeat(32));
    const k2 = deriveOtpKey('11'.repeat(32));
    const k3 = deriveOtpKey('22'.repeat(32));
    expect(Buffer.from(k1)).toEqual(Buffer.from(k2));
    expect(Buffer.from(k1)).not.toEqual(Buffer.from(k3));
  });

  it('otpHmac binds the approvalId (same OTP, different id → different hmac)', () => {
    const key = deriveOtpKey('11'.repeat(32));
    expect(otpHmacHex(key, 'a', '123456')).not.toBe(otpHmacHex(key, 'b', '123456'));
  });

  it('sign/verify round-trips and rejects a tampered payload', () => {
    const key = '33'.repeat(32);
    const pub = publicKeyHex(key);
    const sig = signPayload('hello', key);
    expect(verifyPayloadSig('hello', sig, pub)).toBe(true);
    expect(verifyPayloadSig('hell0', sig, pub)).toBe(false);
  });
});
