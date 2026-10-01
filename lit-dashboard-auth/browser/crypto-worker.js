import { argon2id } from "hash-wasm";
import {
  KDF,
  FORMAT,
  encoder,
  hex,
  unhex,
  randomHex,
  validateParameters,
  validateEnvelope,
  aad,
} from "../../lit-static/dapps/dashboard/password-protocol.js";

export async function derive(password, parameters) {
  validateParameters(parameters);
  if (typeof password !== "string" || encoder.encode(password).length > 1024)
    throw new Error("Password is too long.");
  const passwordBytes = encoder.encode(password);
  let root;
  try {
    root = await argon2id({
      password: passwordBytes,
      salt: unhex(parameters.salt, 16),
      memorySize: KDF.memory,
      iterations: KDF.iterations,
      parallelism: KDF.parallelism,
      hashLength: 32,
      outputType: "binary",
    });
    const source = await crypto.subtle.importKey("raw", root, "HKDF", false, [
      "deriveBits",
    ]);
    const expand = (label) =>
      crypto.subtle.deriveBits(
        {
          name: "HKDF",
          hash: "SHA-256",
          salt: encoder.encode("chipotle-password-v1"),
          info: encoder.encode(
            JSON.stringify([label, parameters.environment, parameters.id]),
          ),
        },
        source,
        256,
      );
    const encryptionBytes = new Uint8Array(await expand("encryption"));
    const encryptionKey = await crypto.subtle.importKey(
      "raw",
      encryptionBytes,
      "AES-GCM",
      false,
      ["encrypt", "decrypt"],
    );
    encryptionBytes.fill(0);
    return {
      encryptionKey,
      authSecret: hex(new Uint8Array(await expand("authentication"))),
    };
  } finally {
    passwordBytes.fill(0);
    root?.fill(0);
  }
}
export async function operate({
  operation,
  password,
  parameters,
  envelope,
  apiKey,
  account,
  newPassword,
}) {
  const keys = await derive(password, parameters);
  if (operation === "derive") return { authSecret: keys.authSecret };
  if (operation === "decrypt" || operation === "change") {
    validateEnvelope(envelope, parameters);
    try {
      const bytes = new Uint8Array(
        await crypto.subtle.decrypt(
          {
            name: "AES-GCM",
            iv: unhex(envelope.nonce, 12),
            additionalData: aad(envelope),
            tagLength: 128,
          },
          keys.encryptionKey,
          unhex(envelope.ciphertext, 60),
        ),
      );
      apiKey = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
      bytes.fill(0);
    } catch {
      throw new Error("Unable to unlock this account. Check your password.");
    }
    if (operation === "decrypt") return { apiKey, authSecret: keys.authSecret };
  }
  if (operation !== "encrypt" && operation !== "change")
    throw new Error("Invalid crypto operation.");
  if (typeof apiKey !== "string" || !/^[A-Za-z0-9+/]{43}=$/.test(apiKey))
    throw new Error("Invalid account API key.");
  const oldAuthSecret = keys.authSecret;
  if (operation === "change") {
    account = envelope.account;
    parameters = {
      ...parameters,
      salt: randomHex(16),
      version: parameters.version + 1,
    };
    Object.assign(keys, await derive(newPassword, parameters));
  }
  const result = {
    format: FORMAT,
    cipher: "AES-256-GCM",
    environment: parameters.environment,
    id: parameters.id,
    salt: parameters.salt,
    version: parameters.version,
    account: account.toLowerCase(),
    nonce: randomHex(12),
    ciphertext: "",
  };
  const plaintext = encoder.encode(apiKey);
  try {
    result.ciphertext = hex(
      new Uint8Array(
        await crypto.subtle.encrypt(
          {
            name: "AES-GCM",
            iv: unhex(result.nonce, 12),
            additionalData: aad(result),
            tagLength: 128,
          },
          keys.encryptionKey,
          plaintext,
        ),
      ),
    );
  } finally {
    plaintext.fill(0);
  }
  validateEnvelope(result, parameters);
  return {
    envelope: result,
    parameters,
    authSecret: keys.authSecret,
    ...(operation === "change" ? { oldAuthSecret } : {}),
  };
}
// Each UI invocation owns a fresh worker, terminated as soon as it returns.
if (typeof self !== "undefined" && typeof self.postMessage === "function") {
  self.onmessage = async ({ data }) => {
    try {
      self.postMessage({ result: await operate(data) });
    } catch (e) {
      self.postMessage({
        error: e instanceof Error ? e.message : "Unable to unlock account.",
      });
    }
  };
}
