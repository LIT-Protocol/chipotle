// lit-action-encryption.js
//
// One-line encrypt/decrypt helpers for Lit Actions. There are two strategies,
// and which one you want depends entirely on payload size:
//
//   Strategy 1 -- IN-ENCLAVE (text + small files).
//   The action calls Lit.Actions.Encrypt / Lit.Actions.Decrypt. AES runs inside
//   the TEE against a symmetric key the node derives from the PKP; that key is
//   NEVER exposed -- only ciphertext/plaintext cross the boundary. Use this when
//   the whole payload comfortably fits through the enclave (a secret, a config
//   blob, a small file). Plaintext only ever exists inside the TEE.
//
//   Strategy 2 -- DERIVED KEY (large files).
//   A file too big to round-trip through the enclave can't use Strategy 1 at
//   all. Instead derive a deterministic AES-256 key from the PKP + a caller
//   identifier, hand the key back, and run the bulk AES-GCM yourself -- in the
//   browser/server, OUTSIDE the enclave. Same pkpId + identifier always yields
//   the same key, so the file stays decryptable later by re-deriving.
//
// We do not hand-roll crypto. keccak-256 + byte/hex utils come from
// @noble/hashes (Paul Miller); AES-GCM is the runtime's WebCrypto (crypto.subtle,
// a Deno global in the action and a Node/browser global on the client). The
// aesGcm* and *WithDerivedKey helpers are pure WebCrypto + the derived key, so
// they run unchanged in the action OR on the client -- the same code decrypts on
// whichever side holds the key.
//
// The bare, version-pinned specifiers below (e.g. "@noble/hashes@1.4.0/...") are
// resolved by the Lit Action runtime's CDN module loader -- see
// docs/lit-actions/imports.mdx. This module is written to live in the
// examples/chipotle-primitives package (whose README covers the bundle / vendor /
// publish options and the barrel export); used standalone, vendor or bundle the
// @noble/hashes import the same way the other examples do.

import { keccak_256 } from "@noble/hashes@1.4.0/sha3/+esm";
import {
  bytesToHex,
  hexToBytes,
  utf8ToBytes,
  concatBytes,
} from "@noble/hashes@1.4.0/utils/+esm";

// 0x-hex (or bare hex) -> Uint8Array. Tolerates an optional 0x prefix so a key
// returned to a client and pasted back round-trips either way.
const fromHex = (hex) => hexToBytes(hex.replace(/^0x/, ""));

// Uint8Array | ArrayBuffer -> Uint8Array, without copying when already a view.
const toBytes = (data) =>
  data instanceof ArrayBuffer ? new Uint8Array(data) : data;

// ---------------------------------------------------------------------------
// Strategy 1 -- in-enclave (Lit.Actions.Encrypt / Decrypt). Action-only.
// ---------------------------------------------------------------------------

/**
 * Encrypt a UTF-8 string to a PKP. The plaintext never leaves the enclave in the
 * clear; only the returned ciphertext does. (Same call as lit-action-secrets'
 * `sealToVault`, named for the text/file family.)
 * @param {{ pkpId: string, text: string }} params
 * @returns {Promise<string>} hex ciphertext (nonce + ciphertext + GCM tag)
 * @throws if text is empty/blank -- the node rejects blank messages, and an
 *   empty payload can't be decrypted back (the node rejects empty plaintext), so
 *   we fail fast here with an action-level error rather than a low-level one.
 */
export async function encryptText({ pkpId, text }) {
  if (!text || text.trim() === "") {
    throw new Error("encryptText: text must be a non-empty string");
  }
  return Lit.Actions.Encrypt({ pkpId, message: text });
}

/**
 * Decrypt a ciphertext produced by encryptText back into text.
 * @param {{ pkpId: string, ciphertext: string }} params
 * @returns {Promise<string>}
 */
export async function decryptText({ pkpId, ciphertext }) {
  return Lit.Actions.Decrypt({ pkpId, ciphertext });
}

/**
 * Encrypt the raw bytes of a small file in-enclave. The bytes are hex-wrapped
 * before encryption so arbitrary binary survives the text-only transport (the
 * node decodes the recovered plaintext as UTF-8, which is lossy for raw bytes
 * but exact for the ASCII hex we send). Use this only for files that fit through
 * the enclave; for large files use deriveEncryptionKey + aesGcmEncrypt instead.
 * @param {{ pkpId: string, bytes: Uint8Array|ArrayBuffer }} params
 * @returns {Promise<string>} hex ciphertext
 * @throws if bytes is empty -- an empty file hex-wraps to "" which the node
 *   rejects (and empty plaintext can't be decrypted back), so fail fast here.
 */
export async function encryptBytes({ pkpId, bytes }) {
  const u8 = toBytes(bytes);
  if (!u8 || u8.length === 0) {
    throw new Error("encryptBytes: bytes must be non-empty");
  }
  return Lit.Actions.Encrypt({ pkpId, message: bytesToHex(u8) });
}

/**
 * Decrypt a ciphertext produced by encryptBytes back into raw bytes.
 * @param {{ pkpId: string, ciphertext: string }} params
 * @returns {Promise<Uint8Array>}
 */
export async function decryptBytes({ pkpId, ciphertext }) {
  return hexToBytes(await Lit.Actions.Decrypt({ pkpId, ciphertext }));
}

// ---------------------------------------------------------------------------
// Strategy 2 -- derived key. deriveEncryptionKey is action-only (it reads the
// PKP); the aesGcm* helpers are portable (action OR client).
// ---------------------------------------------------------------------------

/**
 * Derive a deterministic AES-256 key from the PKP and a caller-chosen
 * identifier, and return it so encryption/decryption can happen OUTSIDE the
 * enclave -- the strategy for files too large to pass through it.
 *
 *   key = keccak256( pkpPrivateKey || keccak256(utf8(identifier)) )
 *
 * The same pkpId + identifier always yields the same key. Because the key is a
 * one-way keccak image, leaking a derived key reveals neither the PKP private
 * key nor any other identifier's key: each derived key is an independent hash
 * output, so multiple leaked keys can't be combined to recover the root (there
 * is no additive/algebraic relationship to exploit). Scope the identifier per
 * file/tenant so a leak is contained to that one payload.
 *
 * @param {{ pkpId: string, identifier: string }} params identifier is required
 * @returns {Promise<{ keyHex: string }>} 32-byte key as bare hex (no 0x)
 */
export async function deriveEncryptionKey({ pkpId, identifier }) {
  if (!identifier) {
    throw new Error("deriveEncryptionKey: identifier is required");
  }
  const pkHex = await Lit.Actions.getPrivateKey({ pkpId });
  const idHash = keccak_256(utf8ToBytes(identifier));
  const keyBytes = keccak_256(concatBytes(fromHex(pkHex), idHash));
  return { keyHex: bytesToHex(keyBytes) };
}

/** Import a hex AES-256 key as a non-extractable WebCrypto AES-GCM key. */
async function importAesKey(keyHex) {
  return crypto.subtle.importKey(
    "raw",
    fromHex(keyHex),
    { name: "AES-GCM" },
    false,
    ["encrypt", "decrypt"],
  );
}

/**
 * AES-256-GCM encrypt bytes with a hex key (e.g. one from deriveEncryptionKey).
 * Pure WebCrypto: runs in the action or on the client. A fresh 96-bit IV is
 * generated per call, so encrypting the same data twice yields distinct output.
 * @param {{ keyHex: string, data: Uint8Array|ArrayBuffer }} params
 * @returns {Promise<{ ciphertext: string, iv: string }>} both hex-encoded
 */
export async function aesGcmEncrypt({ keyHex, data }) {
  const key = await importAesKey(keyHex);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv },
    key,
    toBytes(data),
  );
  return { ciphertext: bytesToHex(new Uint8Array(ct)), iv: bytesToHex(iv) };
}

/**
 * AES-256-GCM decrypt. Throws if the key, IV, or ciphertext don't match (the
 * GCM auth tag fails closed on a wrong key or tampered ciphertext).
 * @param {{ keyHex: string, ciphertext: string, iv: string }} params
 * @returns {Promise<Uint8Array>}
 */
export async function aesGcmDecrypt({ keyHex, ciphertext, iv }) {
  const key = await importAesKey(keyHex);
  const pt = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: fromHex(iv) },
    key,
    fromHex(ciphertext),
  );
  return new Uint8Array(pt);
}

/**
 * Strategy 2 convenience: derive the per-identifier key and AES-GCM encrypt in
 * one call, from inside the action. Only for files that DO fit through the
 * enclave -- for a truly large file, call deriveEncryptionKey, return keyHex to
 * the client, and run aesGcmEncrypt there so the bytes never enter the enclave.
 * @param {{ pkpId: string, identifier: string, data: Uint8Array|ArrayBuffer }} params
 * @returns {Promise<{ ciphertext: string, iv: string }>}
 */
export async function encryptWithDerivedKey({ pkpId, identifier, data }) {
  const { keyHex } = await deriveEncryptionKey({ pkpId, identifier });
  return aesGcmEncrypt({ keyHex, data });
}

/**
 * Inverse of encryptWithDerivedKey: re-derive the key and decrypt.
 * @param {{ pkpId: string, identifier: string, ciphertext: string, iv: string }} params
 * @returns {Promise<Uint8Array>}
 */
export async function decryptWithDerivedKey({
  pkpId,
  identifier,
  ciphertext,
  iv,
}) {
  const { keyHex } = await deriveEncryptionKey({ pkpId, identifier });
  return aesGcmDecrypt({ keyHex, ciphertext, iv });
}
