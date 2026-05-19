// Encrypts your three RPC URLs to the PKP and writes the ciphertexts to
// .env as ENCRYPTED_INFURA_URL / ENCRYPTED_ALCHEMY_URL / ENCRYPTED_QUICKNODE_URL.
// The Lit Action decrypts inside the TEE at execution time — the plaintext
// URLs (which embed provider API keys) never leave the enclave.
//
// The action additionally enforces a hostname whitelist after decryption,
// so even if someone re-encrypts a different URL to the same PKP, the
// action will refuse to use it. Encryption alone is not the trust anchor —
// it's the encryption + whitelist + content-addressed action CID together.
//
// Usage:
//   node scripts/encryptRpcUrls.js
//
// (also invoked by setup.js as part of the one-shot setup)

const env = require("./_env");
env.load();

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_USAGE_API_KEY,
  DECRYPT_PKP_ADDRESS,
  INFURA_URL,
  ALCHEMY_URL,
  QUICKNODE_URL,
} = process.env;

const ENCRYPT_ACTION_CODE = `
  async function main({ pkpId, secret }) {
    const ciphertext = await Lit.Actions.Encrypt({ pkpId, message: secret });
    return { ciphertext };
  }
`;

async function encryptOne(secret) {
  const res = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
    method: "POST",
    headers: {
      "X-Api-Key": LIT_USAGE_API_KEY,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      code: ENCRYPT_ACTION_CODE,
      js_params: { pkpId: DECRYPT_PKP_ADDRESS, secret },
    }),
  });
  const body = await res.json();
  if (!body.ciphertext) {
    throw new Error(`encrypt failed: ${JSON.stringify(body)}`);
  }
  return body.ciphertext;
}

async function encryptRpcUrls() {
  for (const k of [
    "LIT_USAGE_API_KEY",
    "DECRYPT_PKP_ADDRESS",
    "INFURA_URL",
    "ALCHEMY_URL",
    "QUICKNODE_URL",
  ]) {
    if (!process.env[k]) throw new Error(`${k} is required`);
  }

  // Encrypt in parallel since these are independent.
  const [infura, alchemy, quicknode] = await Promise.all([
    encryptOne(INFURA_URL),
    encryptOne(ALCHEMY_URL),
    encryptOne(QUICKNODE_URL),
  ]);

  env.upsert("ENCRYPTED_INFURA_URL", infura);
  env.upsert("ENCRYPTED_ALCHEMY_URL", alchemy);
  env.upsert("ENCRYPTED_QUICKNODE_URL", quicknode);
  return { infura, alchemy, quicknode };
}

if (require.main === module) {
  encryptRpcUrls()
    .then(({ infura, alchemy, quicknode }) => {
      console.log(`ENCRYPTED_INFURA_URL=${infura.slice(0, 40)}...`);
      console.log(`ENCRYPTED_ALCHEMY_URL=${alchemy.slice(0, 40)}...`);
      console.log(`ENCRYPTED_QUICKNODE_URL=${quicknode.slice(0, 40)}...`);
      console.log("(written to .env)");
    })
    .catch((err) => {
      console.error(err.message);
      process.exit(1);
    });
}

module.exports = { encryptRpcUrls };
