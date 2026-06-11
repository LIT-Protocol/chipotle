import { describe, expect, it } from 'vitest';
import { sha256 } from '@noble/hashes/sha256';
import { utf8ToBytes } from '@noble/hashes/utils';
import { p256 } from '@noble/curves/p256';
import {
  b64decode,
  b64encode,
  ed25519SignBase64,
  es256Jwt,
  hmacSha256Hex,
  parseEcP256PrivateKey,
  parseEd25519PrivateKey,
} from '../src/signing';

describe('hmacSha256Hex', () => {
  it('matches the Binance API docs signed-endpoint example (verified against openssl)', () => {
    const secret = 'NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j';
    const payload =
      'symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559';
    expect(hmacSha256Hex(secret, payload)).toBe(
      'c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71',
    );
  });
});

describe('base64', () => {
  it('round-trips and matches Buffer for lengths 0..17', () => {
    for (let len = 0; len <= 17; len++) {
      const bytes = new Uint8Array(len).map((_, i) => (i * 37 + len * 11) & 0xff);
      const ours = b64encode(bytes);
      expect(ours).toBe(Buffer.from(bytes).toString('base64'));
      expect(Buffer.from(b64decode(ours))).toEqual(Buffer.from(bytes));
    }
  });
});

describe('ed25519SignBase64', () => {
  // RFC 8032 §7.1 TEST 2: one-byte message 0x72 ('r').
  const seedHex = '4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb';
  const sigHex =
    '92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da' +
    '085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00';
  const expectedB64 = Buffer.from(sigHex, 'hex').toString('base64');

  it('signs with a raw hex seed (RFC 8032 test vector)', () => {
    expect(ed25519SignBase64(seedHex, 'r')).toBe(expectedB64);
  });

  it('signs with a PKCS8 PEM key (parses the DER wrapper)', () => {
    const der = Buffer.from('302e020100300506032b657004220420' + seedHex, 'hex');
    const pem = `-----BEGIN PRIVATE KEY-----\n${der.toString('base64')}\n-----END PRIVATE KEY-----`;
    expect(ed25519SignBase64(pem, 'r')).toBe(expectedB64);
    expect(Buffer.from(parseEd25519PrivateKey(pem)).toString('hex')).toBe(seedHex);
  });

  it('rejects garbage keys', () => {
    expect(() => parseEd25519PrivateKey('not a key')).toThrow();
  });
});

describe('es256Jwt (Coinbase CDP auth)', () => {
  const scalarHex = '0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20';
  const scalar = Buffer.from(scalarHex, 'hex');
  // SEC1 ECPrivateKey: SEQ { INTEGER 1, OCTET STRING(32), [0] OID prime256v1 }
  const sec1 = Buffer.concat([
    Buffer.from('30310201010420', 'hex'),
    scalar,
    Buffer.from('a00a06082a8648ce3d030107', 'hex'),
  ]);
  const pem = `-----BEGIN EC PRIVATE KEY-----\n${sec1.toString('base64')}\n-----END EC PRIVATE KEY-----`;
  const keyName = 'organizations/abc/apiKeys/def';
  const uri = 'GET api.coinbase.com/api/v3/brokerage/accounts';

  it('parses SEC1 PEM and raw hex to the same scalar', () => {
    expect(Buffer.from(parseEcP256PrivateKey(pem)).toString('hex')).toBe(scalarHex);
    expect(Buffer.from(parseEcP256PrivateKey(scalarHex)).toString('hex')).toBe(scalarHex);
  });

  it('parses PEMs whose newlines arrive as literal \\n sequences (CDP JSON download format)', () => {
    const literal = pem.replace(/\n/g, '\\n');
    expect(literal).not.toContain('\n');
    expect(Buffer.from(parseEcP256PrivateKey(literal)).toString('hex')).toBe(scalarHex);
  });

  it('produces a verifiable JWT with the CDP claim shape', () => {
    const jwt = es256Jwt({
      keyName,
      privateKey: pem,
      uri,
      nowMs: 1_700_000_000_000,
      nonce: 'deadbeef',
    });
    const [h, p, s] = jwt.split('.') as [string, string, string];
    const header = JSON.parse(Buffer.from(h, 'base64url').toString());
    expect(header).toEqual({ alg: 'ES256', kid: keyName, nonce: 'deadbeef', typ: 'JWT' });
    const payload = JSON.parse(Buffer.from(p, 'base64url').toString());
    expect(payload.iss).toBe('cdp');
    expect(payload.sub).toBe(keyName);
    expect(payload.uri).toBe(uri);
    expect(payload.nbf).toBe(1_700_000_000);
    expect(payload.exp - payload.nbf).toBe(120);

    const pub = p256.getPublicKey(scalar);
    const ok = p256.verify(
      new Uint8Array(Buffer.from(s, 'base64url')),
      sha256(utf8ToBytes(`${h}.${p}`)),
      pub,
    );
    expect(ok).toBe(true);
  });
});
