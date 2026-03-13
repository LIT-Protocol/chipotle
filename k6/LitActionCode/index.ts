/**
 * Shared Lit Action code for k6 tests.
 * Source files: hello-world.js, encrypt.js, decrypt.js in this directory.
 */
export const HELLO_WORLD_CODE = 'Lit.Actions.setResponse({response: "Hello World!"})';

export const ENCRYPT_CODE = `(async () => {
  const ciphertext = await Lit.Actions.Encrypt({ pkpId, message: challenge });
  Lit.Actions.setResponse({ response: ciphertext });
})();`;

export const DECRYPT_CODE = `(async () => {
  const plaintext = await Lit.Actions.Decrypt({ pkpId, ciphertext });
  Lit.Actions.setResponse({ response: plaintext });
})();`;
