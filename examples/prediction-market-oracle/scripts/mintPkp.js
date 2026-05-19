// Mints a fresh PKP and writes the address to .env as DECRYPT_PKP_ADDRESS.
//
// This PKP is used solely as the encryption boundary for the AI provider
// API keys (Perplexity, optionally OpenAI and Anthropic). The signature
// the on-chain PredictionMarket trusts is produced by the action's own
// derived key — see scripts/setup.js for the full picture.
//
// Usage:
//   node scripts/mintPkp.js
//
// (also invoked by setup.js)

const env = require("./_env");
env.load();

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_API_KEY,
} = process.env;

async function mintPkp() {
  if (!LIT_API_KEY) throw new Error("LIT_API_KEY is required");

  const res = await fetch(`${LIT_API_BASE}/core/v1/create_wallet`, {
    method: "GET",
    headers: { "X-Api-Key": LIT_API_KEY },
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
