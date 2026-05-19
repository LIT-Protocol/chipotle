// Mints a fresh PKP and writes the address to .env as DECRYPT_PKP_ADDRESS.
//
// This PKP is used *only* as the encryption boundary for the three RPC
// URLs — Encrypt/Decrypt in Lit are PKP-keyed, and we need somewhere for
// the ciphertexts to live that only this action's TEE can decrypt. The
// PKP does not sign anything the registry cares about. The signature the
// registry verifies is produced by the action's own derived key
// (Lit.Actions.getLitActionPrivateKey), which is bound to the action's
// IPFS CID.
//
// Usage:
//   node scripts/mintPkp.js
//
// (also invoked by setup.js as part of the one-shot setup)

const env = require("./_env");
env.load();

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_USAGE_API_KEY,
} = process.env;

async function mintPkp() {
  if (!LIT_USAGE_API_KEY) throw new Error("LIT_USAGE_API_KEY is required");

  const res = await fetch(`${LIT_API_BASE}/core/v1/create_wallet`, {
    method: "GET",
    headers: { "X-Api-Key": LIT_USAGE_API_KEY },
  });
  const body = await res.json();
  if (!res.ok || !body.wallet_address) {
    throw new Error(`mint failed: ${JSON.stringify(body)}`);
  }
  env.upsert("DECRYPT_PKP_ADDRESS", body.wallet_address);
  return body.wallet_address;
}

if (require.main === module) {
  mintPkp()
    .then((addr) => console.log(`DECRYPT_PKP_ADDRESS=${addr} (written to .env)`))
    .catch((err) => {
      console.error(err.message);
      process.exit(1);
    });
}

module.exports = { mintPkp };
