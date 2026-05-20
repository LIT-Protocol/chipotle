// End-to-end resolution:
//   1. Read the question text + resolveAt from the on-chain PredictionMarket.
//   2. Ask the Lit Action to poll all configured models, require consensus,
//      and sign the resolution.
//   3. Submit `resolve(id, answer, deadline, sig)` on-chain.
//
// Usage:
//   node scripts/resolve.js --id 0xQuestionHash

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");
env.load();

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_USAGE_API_KEY,
  DECRYPT_PKP_ADDRESS,
  PREDICTION_MARKET_ADDRESS,
  ENCRYPTED_PERPLEXITY_API_KEY,
  ENCRYPTED_OPENAI_API_KEY,
  ENCRYPTED_ANTHROPIC_API_KEY,
  CHAIN_ID = "84532",
  RPC_URL = "https://sepolia.base.org",
  RESOLVER_PRIVATE_KEY,
} = process.env;

const ANSWER_NAMES = ["Unresolved", "Yes", "No", "Unclear"];

function parseArgs() {
  const out = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    out[process.argv[i].replace(/^--/, "")] = process.argv[i + 1];
  }
  return out;
}

async function main() {
  const args = parseArgs();
  if (!args.id) throw new Error("Usage: node scripts/resolve.js --id 0x...");
  for (const k of [
    "LIT_USAGE_API_KEY",
    "DECRYPT_PKP_ADDRESS",
    "PREDICTION_MARKET_ADDRESS",
    "ENCRYPTED_PERPLEXITY_API_KEY",
    "RESOLVER_PRIVATE_KEY",
  ]) {
    if (!process.env[k]) throw new Error(`${k} is required`);
  }

  const provider = new ethers.providers.JsonRpcProvider(RPC_URL);
  const resolver = new ethers.Wallet(RESOLVER_PRIVATE_KEY, provider);
  const market = new ethers.Contract(
    PREDICTION_MARKET_ADDRESS,
    [
      "function questions(bytes32) view returns (string text, uint256 resolveAt, address proposer, uint8 answer, uint64 resolvedAt)",
      "function resolve(bytes32 id, uint8 answer, uint256 deadline, bytes signature)",
    ],
    resolver
  );

  const q = await market.questions(args.id);
  if (q.resolveAt.eq(0)) throw new Error("question not found");
  if (q.answer !== 0) {
    console.log(`Already resolved: ${ANSWER_NAMES[q.answer]}`);
    process.exit(0);
  }
  console.log("Question:", q.text);
  console.log("resolveAt:", new Date(Number(q.resolveAt) * 1000).toISOString());

  const deadline = Math.floor(Date.now() / 1000) + 600;

  const code = fs.readFileSync(
    path.join(__dirname, "..", "action", "marketOracle.js"),
    "utf8"
  );

  console.log("Asking the AI consensus oracle...");
  const litRes = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
    method: "POST",
    headers: {
      "X-Api-Key": LIT_USAGE_API_KEY,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      code,
      js_params: {
        questionId: args.id,
        questionText: q.text,
        resolveAt: Number(q.resolveAt),
        marketAddress: PREDICTION_MARKET_ADDRESS,
        marketChainId: Number(CHAIN_ID),
        deadline,
        decryptPkpId: DECRYPT_PKP_ADDRESS,
        encryptedPerplexityKey: ENCRYPTED_PERPLEXITY_API_KEY,
        encryptedOpenAiKey: ENCRYPTED_OPENAI_API_KEY || null,
        encryptedAnthropicKey: ENCRYPTED_ANTHROPIC_API_KEY || null,
      },
    }),
  });

  // /lit_action wraps the action's return value as
  //   { response: <whatever you returned>, logs: "...", has_error: bool }
  const envelope = await litRes.json();
  if (envelope.has_error) {
    console.error("Lit Action errored:", envelope.logs || envelope);
    process.exit(2);
  }
  const body = envelope.response;
  if (!body || !body.authorized) {
    console.error("Action declined to sign:", body || envelope);
    process.exit(2);
  }
  console.log(
    `Consensus: ${body.answerName} (across ${body.consensusAcross.join(", ")})`
  );
  if (body.failedModels && body.failedModels.length) {
    console.log("Failed models:", body.failedModels);
  }

  const tx = await market.resolve(args.id, body.answer, deadline, body.signature);
  console.log("tx:", tx.hash);
  const receipt = await tx.wait();
  console.log("mined in block", receipt.blockNumber);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
