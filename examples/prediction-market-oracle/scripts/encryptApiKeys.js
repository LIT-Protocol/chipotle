// Encrypts the AI provider API keys to the decrypt PKP and writes the
// ciphertexts to .env. Perplexity is required (web-grounded baseline);
// OpenAI and Anthropic are optional.
//
// Why this layout: the action's hostname for each model is hard-coded in
// the action source, so swapping in an attacker's "free" API key doesn't
// redirect traffic. The encryption keeps the *user's* paid keys out of
// the action source (which is publicly content-addressed) and out of any
// caller-controlled js_params at runtime.
//
// Usage:
//   node scripts/encryptApiKeys.js
//
// (also invoked by setup.js)

const env = require("./_env");
env.load();

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_USAGE_API_KEY,
  DECRYPT_PKP_ADDRESS,
  PERPLEXITY_API_KEY,
  OPENAI_API_KEY,
  ANTHROPIC_API_KEY,
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

async function encryptApiKeys() {
  for (const k of ["LIT_USAGE_API_KEY", "DECRYPT_PKP_ADDRESS", "PERPLEXITY_API_KEY"]) {
    if (!process.env[k]) throw new Error(`${k} is required`);
  }

  const out = {};

  out.perplexity = await encryptOne(PERPLEXITY_API_KEY);
  env.upsert("ENCRYPTED_PERPLEXITY_API_KEY", out.perplexity);
  console.log(`  Perplexity: encrypted (${out.perplexity.slice(0, 40)}...)`);

  if (OPENAI_API_KEY) {
    out.openai = await encryptOne(OPENAI_API_KEY);
    env.upsert("ENCRYPTED_OPENAI_API_KEY", out.openai);
    console.log(`  OpenAI:     encrypted (${out.openai.slice(0, 40)}...)`);
  } else {
    console.log("  OpenAI:     skipped (no OPENAI_API_KEY in .env)");
  }

  if (ANTHROPIC_API_KEY) {
    out.anthropic = await encryptOne(ANTHROPIC_API_KEY);
    env.upsert("ENCRYPTED_ANTHROPIC_API_KEY", out.anthropic);
    console.log(`  Anthropic:  encrypted (${out.anthropic.slice(0, 40)}...)`);
  } else {
    console.log("  Anthropic:  skipped (no ANTHROPIC_API_KEY in .env)");
  }

  return out;
}

if (require.main === module) {
  encryptApiKeys()
    .then(() => console.log("(written to .env)"))
    .catch((err) => {
      console.error(err.message);
      process.exit(1);
    });
}

module.exports = { encryptApiKeys };
