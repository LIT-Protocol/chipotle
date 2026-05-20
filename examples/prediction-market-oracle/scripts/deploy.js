// Deploys PredictionMarket with the action's derived wallet address pinned
// as the oracle, and writes the deployed address to .env as
// PREDICTION_MARKET_ADDRESS.
//
// Usage:
//   ACTION_WALLET_ADDRESS=0x... npx hardhat run scripts/deploy.js --network baseSepolia
//
// (also invoked by setup.js via execSync)

const hre = require("hardhat");
const env = require("./_env");

async function main() {
  env.load();
  const oracle = process.env.ACTION_WALLET_ADDRESS;
  if (!oracle) throw new Error("ACTION_WALLET_ADDRESS env var is required");

  const factory = await hre.ethers.getContractFactory("PredictionMarket");
  const market = await factory.deploy(oracle);
  await market.deployed();

  const address = market.address;
  console.log("PredictionMarket deployed:", address);
  console.log("Oracle (action address):", oracle);

  env.upsert("PREDICTION_MARKET_ADDRESS", address);
  console.log("Wrote PREDICTION_MARKET_ADDRESS to .env");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
