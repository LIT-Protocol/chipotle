// Proposes a question to the deployed PredictionMarket contract.
// resolveAt defaults to "now" so you can resolve it as soon as the
// propose tx mines. A real prediction market would set this to whenever
// the underlying event is expected to be decidable; for a docs demo,
// immediate is fine.
//
// Usage:
//   node scripts/propose.js --text "Did the Lakers beat the Celtics on 2026-05-15?"
//   node scripts/propose.js --text "..." --resolveIn 3600   # 1 hour from now

const { ethers } = require("ethers");
const env = require("./_env");
env.load();

const {
  PREDICTION_MARKET_ADDRESS,
  RPC_URL = "https://sepolia.base.org",
  PROPOSER_PRIVATE_KEY,
} = process.env;

function parseArgs() {
  const out = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    out[process.argv[i].replace(/^--/, "")] = process.argv[i + 1];
  }
  return out;
}

async function main() {
  const args = parseArgs();
  if (!args.text) {
    throw new Error('Usage: node scripts/propose.js --text "..." [--resolveIn 300]');
  }
  for (const k of ["PREDICTION_MARKET_ADDRESS", "PROPOSER_PRIVATE_KEY"]) {
    if (!process.env[k]) throw new Error(`${k} is required`);
  }

  // Default is 0 — question is resolvable as soon as the propose tx
  // mines. Pass `--resolveIn N` for a longer window (real prediction
  // markets would set this to whenever the underlying event closes).
  const resolveIn = Number(args.resolveIn ?? "0");
  const resolveAt = Math.floor(Date.now() / 1000) + resolveIn;

  const provider = new ethers.providers.JsonRpcProvider(RPC_URL);
  const wallet = new ethers.Wallet(PROPOSER_PRIVATE_KEY, provider);
  const market = new ethers.Contract(
    PREDICTION_MARKET_ADDRESS,
    [
      "function propose(string text, uint256 resolveAt) returns (bytes32)",
      "function questionId(string text) view returns (bytes32)",
      "event QuestionProposed(bytes32 indexed id, address indexed proposer, string text, uint256 resolveAt)",
    ],
    wallet
  );

  const id = await market.questionId(args.text);
  console.log("Question text:", args.text);
  console.log("Question id:  ", id);
  console.log("resolveAt:    ", new Date(resolveAt * 1000).toISOString(), `(in ${resolveIn}s)`);

  const tx = await market.propose(args.text, resolveAt);
  console.log("tx:", tx.hash);
  const receipt = await tx.wait();
  console.log("mined in block", receipt.blockNumber);
  console.log("\nResolve with:");
  console.log(`  npm run resolve -- --id ${id}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
