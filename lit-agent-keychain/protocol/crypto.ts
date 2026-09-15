import { sha256 } from "@noble/hashes/sha2.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { secp256k1 } from "@noble/curves/secp256k1.js";
import { ed25519, x25519 } from "@noble/curves/ed25519.js";
import { CipherSuite, Aes256Gcm, HkdfSha256 } from "@hpke/core";
import { DhkemX25519HkdfSha256 } from "@hpke/dhkem-x25519";
import {
  DOMAIN,
  V,
  MAX_SECRET_BYTES,
  envelopeSchema,
  type Envelope,
  type Receipt,
  type Signed,
  type ReadRequest,
} from "./schema.ts";
export const utf8 = (s: string) => new TextEncoder().encode(s);
export const decode = (b: Uint8Array) =>
  new TextDecoder("utf-8", { fatal: true }).decode(b);
export const hex = (b: Uint8Array) =>
  Array.from(b, (n) => n.toString(16).padStart(2, "0")).join("");
export function unhex(s: string): Uint8Array<ArrayBuffer> {
  if (!/^(?:[0-9a-f]{2})+$/.test(s)) throw new Error("Invalid hex");
  return Uint8Array.from(s.match(/../g)!.map((n) => parseInt(n, 16)));
}
export function b64u(b: Uint8Array): string {
  let s = "";
  for (const n of b) s += String.fromCharCode(n);
  return btoa(s).replace(/=/g, "").replace(/\+/g, "-").replace(/\//g, "_");
}
export function unb64u(s: string, max = 32768): Uint8Array<ArrayBuffer> {
  if (
    typeof s !== "string" ||
    s.length > Math.ceil((max * 4) / 3) ||
    !/^[A-Za-z0-9_-]+$/.test(s)
  )
    throw new Error("Invalid base64url");
  const b = Uint8Array.from(
    atob(s.replace(/-/g, "+").replace(/_/g, "/")),
    (c) => c.charCodeAt(0),
  );
  if (b.length > max || b64u(b) !== s)
    throw new Error("Non-canonical base64url");
  return b;
}
// Restricted canonical JSON: sorted ASCII field names; safe integer numbers only.
// Protocol documents never contain floats, undefined, prototypes or arbitrary keys.
export function canonical(value: unknown, depth = 0): string {
  if (depth > 24) throw new Error("Object too deep");
  if (value === null || typeof value === "boolean" || typeof value === "string")
    return JSON.stringify(value);
  if (
    typeof value === "number" &&
    Number.isSafeInteger(value) &&
    !Object.is(value, -0)
  )
    return String(value);
  if (Array.isArray(value))
    return "[" + value.map((v) => canonical(v, depth + 1)).join(",") + "]";
  if (
    value &&
    typeof value === "object" &&
    Object.getPrototypeOf(value) === Object.prototype
  ) {
    const entries = Object.entries(value).sort(([a], [b]) =>
      a < b ? -1 : a > b ? 1 : 0,
    );
    if (entries.some(([k]) => !/^[A-Za-z][A-Za-z0-9]*$/.test(k)))
      throw new Error("Invalid field name");
    return (
      "{" +
      entries
        .map(([k, v]) => JSON.stringify(k) + ":" + canonical(v, depth + 1))
        .join(",") +
      "}"
    );
  }
  throw new Error("Not a canonical protocol value");
}
export const digest = (v: unknown) => hex(sha256(utf8(canonical(v))));
export const randomBytes = (n = 32) =>
  crypto.getRandomValues(new Uint8Array(n));
export const randomId = () => hex(randomBytes());
export const nowSeconds = () => Math.floor(Date.now() / 1000);
export function requireThat(
  ok: unknown,
  message = "Authorization denied",
): asserts ok {
  if (!ok) throw new Error(message);
}
export function signAction(payload: unknown, key: Uint8Array): string {
  return hex(
    secp256k1.sign(unhex(digest(payload)), key, {
      prehash: false,
      format: "compact",
    }),
  );
}
export function verifyAction(
  payload: unknown,
  signature: string,
  publicKey: string,
): void {
  requireThat(
    secp256k1.verify(
      unhex(signature),
      unhex(digest(payload)),
      unhex(publicKey.replace(/^0x/, "")),
      { prehash: false, format: "compact", lowS: true },
    ),
  );
}
export function verifyReceipt<T>(
  signed: Signed<T>,
  publicKey: string,
  vaultId: string,
): void {
  requireThat(
    signed.receipt.payload.v === V &&
      signed.receipt.payload.domain === "lit-keychain/receipt/v2" &&
      signed.receipt.payload.vaultId === vaultId &&
      signed.receipt.payload.objectHash === digest(signed.document),
  );
  verifyAction(signed.receipt.payload, signed.receipt.signature, publicKey);
}
export const agentPublicKey = (key: Uint8Array) =>
  hex(ed25519.getPublicKey(key));
export const signAgent = (payload: unknown, key: Uint8Array) =>
  hex(ed25519.sign(unhex(digest(payload)), key));
export function verifyAgent(
  payload: unknown,
  signature: string,
  key: string,
): void {
  requireThat(
    ed25519.verify(unhex(signature), unhex(digest(payload)), unhex(key), {
      zip215: false,
    }),
  );
}
export function encryptionKey(actionKey: Uint8Array): Uint8Array<ArrayBuffer> {
  return new Uint8Array(
    hkdf(sha256, actionKey, utf8(DOMAIN), utf8("secret-envelope-x25519"), 32),
  );
}
export const encryptionPublicKey = (key: Uint8Array) =>
  hex(x25519.getPublicKey(key));
const suite = new CipherSuite({
  kem: new DhkemX25519HkdfSha256(),
  kdf: new HkdfSha256(),
  aead: new Aes256Gcm(),
});
export async function seal(
  publicKey: string,
  plaintext: Uint8Array<ArrayBuffer>,
  context: unknown,
) {
  const recipientPublicKey = await suite.kem.deserializePublicKey(
    unhex(publicKey).buffer,
  );
  const result = await suite.seal(
    { recipientPublicKey, info: utf8(canonical(context)).buffer },
    plaintext.buffer,
  );
  return {
    enc: b64u(new Uint8Array(result.enc)),
    ciphertext: b64u(new Uint8Array(result.ct)),
  };
}
export async function open(
  key: Uint8Array<ArrayBuffer>,
  sealed: { enc: string; ciphertext: string },
  context: unknown,
) {
  const recipientKey = await suite.kem.deserializePrivateKey(key.buffer);
  return new Uint8Array(
    await suite.open(
      {
        recipientKey,
        enc: unb64u(sealed.enc, 32).buffer,
        info: utf8(canonical(context)).buffer,
      },
      unb64u(sealed.ciphertext).buffer,
    ),
  );
}
export async function encryptEnvelope(
  metadata: Envelope["metadata"],
  publicKey: string,
  plaintext: string,
): Promise<Envelope> {
  const bytes = utf8(plaintext);
  requireThat(
    bytes.length > 0 && bytes.length <= MAX_SECRET_BYTES,
    "Secret must be 1–16384 UTF-8 bytes",
  );
  const dek = randomBytes();
  const iv = randomBytes(12);
  try {
    const key = await crypto.subtle.importKey("raw", dek, "AES-GCM", false, [
      "encrypt",
    ]);
    const ciphertext = new Uint8Array(
      await crypto.subtle.encrypt(
        { name: "AES-GCM", iv, additionalData: utf8(canonical(metadata)) },
        key,
        bytes,
      ),
    );
    const wrapped = await seal(publicKey, dek, {
      domain: "lit-keychain/wrap/v2",
      metadata,
    });
    return envelopeSchema.parse({
      v: V,
      domain: DOMAIN,
      vaultId: metadata.vaultId,
      kind: "envelope",
      metadata,
      enc: wrapped.enc,
      wrappedKey: wrapped.ciphertext,
      iv: b64u(iv),
      ciphertext: b64u(ciphertext),
    });
  } finally {
    dek.fill(0);
    bytes.fill(0);
  }
}
export async function decryptEnvelope(
  envelope: Envelope,
  key: Uint8Array<ArrayBuffer>,
): Promise<Uint8Array<ArrayBuffer>> {
  const e = envelopeSchema.parse(envelope);
  const dek = await open(
    key,
    { enc: e.enc, ciphertext: e.wrappedKey },
    { domain: "lit-keychain/wrap/v2", metadata: e.metadata },
  );
  try {
    requireThat(dek.length === 32);
    const imported = await crypto.subtle.importKey(
      "raw",
      dek,
      "AES-GCM",
      false,
      ["decrypt"],
    );
    return new Uint8Array(
      await crypto.subtle.decrypt(
        {
          name: "AES-GCM",
          iv: unb64u(e.iv, 12),
          additionalData: utf8(canonical(e.metadata)),
        },
        imported,
        unb64u(e.ciphertext, MAX_SECRET_BYTES + 16),
      ),
    );
  } finally {
    dek.fill(0);
  }
}
export const responseContext = (r: ReadRequest) => ({
  domain: "lit-keychain/response-encryption/v2",
  requestHash: digest(r),
});
export function makeReceipt(
  document: { vaultId: string },
  key: Uint8Array,
  now: number,
): Receipt {
  const payload = {
    v: V,
    domain: "lit-keychain/receipt/v2" as const,
    vaultId: document.vaultId,
    objectHash: digest(document),
    issuedAt: now,
  };
  return { payload, signature: signAction(payload, key) };
}
