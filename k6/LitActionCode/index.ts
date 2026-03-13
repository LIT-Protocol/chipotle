/**
 * Shared Lit Action code for k6 tests.
 * Source files: hello-world.js, encrypt.js, decrypt.js, ecdsa-sign.js in this directory.
 * Uses import.meta.resolve() so paths work regardless of cwd.
 */
export const HELLO_WORLD_CODE = open(import.meta.resolve("./hello-world.js"));
export const ECDSA_SIGN_CODE = open(import.meta.resolve("./ecdsa-sign.js"));
export const ENCRYPT_CODE = open(import.meta.resolve("./encrypt.js"));
export const DECRYPT_CODE = open(import.meta.resolve("./decrypt.js"));
