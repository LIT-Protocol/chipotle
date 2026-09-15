// lit-action-secrets.js
//
// Encryption / secrets (PKP-as-vault). Thin wrappers over Lit.Actions.Encrypt /
// Decrypt, plus a decrypt-use-discard helper so a plaintext secret only lives
// inside the TEE for the lifetime of a callback.

/** AES-encrypt plaintext to a PKP; store the ciphertext anywhere. */
export async function sealToVault({ pkpId, plaintext }) {
  return Lit.Actions.Encrypt({ pkpId, message: plaintext });
}

/** Decrypt ciphertext sealed to a PKP. Gate this behind your access logic. */
export async function openFromVault({ pkpId, ciphertext }) {
  return Lit.Actions.Decrypt({ pkpId, ciphertext });
}

/**
 * Decrypt-use-discard wrapper: the secret only exists in plaintext inside the
 * TEE for the lifetime of `fn`. Returns whatever `fn` returns.
 */
export async function withSecret({ pkpId, ciphertext }, fn) {
  const secret = await Lit.Actions.Decrypt({ pkpId, ciphertext });
  return fn(secret);
}

/**
 * Decrypt the secret portion of a provider URL in-TEE and append it to a
 * HARDCODED baseUrl, so the target chain/host stays verifiable from the source.
 * `baseUrl` must come from action constants, never js_params.
 */
export async function assembleSecretRpcUrl({ pkpId, encryptedKey, baseUrl }) {
  const key = await Lit.Actions.Decrypt({ pkpId, ciphertext: encryptedKey });
  return `${baseUrl}${key}`;
}
