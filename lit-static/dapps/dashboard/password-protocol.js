/** Versioned browser/Worker wire format. Never accept caller-selected KDF costs. */
export const KDF = Object.freeze({
  algorithm: "argon2id",
  version: 19,
  memory: 65536,
  iterations: 3,
  parallelism: 4,
});
export const FORMAT = 1;
export const encoder = new TextEncoder();
export function hex(bytes) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
export function unhex(value, length) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]+$/.test(value) ||
    value.length !== length * 2
  )
    throw new Error("Invalid encrypted record.");
  return Uint8Array.from(value.match(/../g), (b) => parseInt(b, 16));
}
export function randomHex(length) {
  return hex(crypto.getRandomValues(new Uint8Array(length)));
}
export function validateParameters(p) {
  if (
    !p ||
    p.format !== FORMAT ||
    typeof p.environment !== "string" ||
    !/^[a-z0-9-]{1,40}$/.test(p.environment)
  )
    throw new Error("Unsupported encryption format.");
  unhex(p.id, 16);
  unhex(p.salt, 16);
  if (!p.kdf || Object.keys(KDF).some((k) => p.kdf[k] !== KDF[k]))
    throw new Error("Unsupported password derivation parameters.");
  if (!Number.isSafeInteger(p.version) || p.version < 1)
    throw new Error("Invalid credential version.");
}
export function validateEnvelope(e, p) {
  validateParameters(p);
  if (
    !e ||
    e.format !== FORMAT ||
    e.cipher !== "AES-256-GCM" ||
    e.id !== p.id ||
    e.environment !== p.environment ||
    e.version !== p.version ||
    e.salt !== p.salt
  )
    throw new Error("Encrypted record does not match this account.");
  if (typeof e.account !== "string" || !/^0x[0-9a-f]{40}$/.test(e.account))
    throw new Error("Invalid account binding.");
  unhex(e.nonce, 12);
  // Current API key is base64 of 32 bytes (44 UTF-8 bytes) plus a 16-byte GCM tag.
  unhex(e.ciphertext, 60);
  return e;
}
export function aad(e) {
  return encoder.encode(
    JSON.stringify([
      "chipotle-api-key",
      FORMAT,
      e.environment,
      e.id,
      e.account,
      e.version,
      e.salt,
    ]),
  );
}
export async function sha256(value) {
  return hex(
    new Uint8Array(
      await crypto.subtle.digest("SHA-256", encoder.encode(value)),
    ),
  );
}
