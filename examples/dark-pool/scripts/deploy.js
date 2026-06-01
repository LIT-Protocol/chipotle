// Deploys the two demo TestTokens (base + quote) and DarkPoolSettlement, pinning
// the matchEpoch action's derived address as the trusted matcher.
//
// Run via setup.js (when DEPLOYER_PRIVATE_KEY is set), or directly:
//   npx hardhat run scripts/deploy.js --network baseSepolia

const hre = require("hardhat");
const env = require("./_env");

async function main() {
  env.load();
  const matcher = process.env.MATCH_ACTION_ADDRESS;
  if (!matcher) throw new Error("MATCH_ACTION_ADDRESS is required (run `npm run setup` first)");
  const pair = process.env.PAIR || "BASE/QUOTE";

  const Token = await hre.ethers.getContractFactory("TestToken");
  const base = await Token.deploy("Dark Base", "dBASE");
  await base.deployed();
  const quote = await Token.deploy("Dark Quote", "dQUOTE");
  await quote.deployed();

  const Settlement = await hre.ethers.getContractFactory("DarkPoolSettlement");
  const settlement = await Settlement.deploy(base.address, quote.address, pair, matcher);
  await settlement.deployed();

  console.log("  Base token:       ", base.address);
  console.log("  Quote token:      ", quote.address);
  console.log("  DarkPoolSettlement:", settlement.address);
  console.log("  Matcher pinned:   ", matcher);

  env.upsert("BASE_TOKEN_ADDRESS", base.address);
  env.upsert("QUOTE_TOKEN_ADDRESS", quote.address);
  env.upsert("SETTLEMENT_ADDRESS", settlement.address);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
