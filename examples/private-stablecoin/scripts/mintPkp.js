// Mints a fresh PKP and writes the address to .env as LEDGER_PKP_ADDRESS.
//
// This PKP is the encryption boundary for the ledger. Every private note's
// contents (owner, amount, salt) are encrypted to it via Lit.Actions.Encrypt
// inside the ledger action, and decrypted via Lit.Actions.Decrypt inside the
// ledger / disclose actions. Only actions authorized in the group (see
// setup.js) can use it, so plaintext note contents never leave a Lit TEE.
//
// Note: this PKP is NOT what the on-chain PrivUSD contract trusts. The
// contract trusts the action's CID-derived signer (ACTION_WALLET_ADDRESS).
// The PKP is purely the encrypt/decrypt key.
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
  env.upsert("LEDGER_PKP_ADDRESS", body.wallet_address);
  return body.wallet_address;
}

if (require.main === module) {
  mintPkp()
    .then((addr) => console.log(`LEDGER_PKP_ADDRESS=${addr} (written to .env)`))
    .catch((err) => {
      console.error(err.message);
      process.exit(1);
    });
}

module.exports = { mintPkp };
