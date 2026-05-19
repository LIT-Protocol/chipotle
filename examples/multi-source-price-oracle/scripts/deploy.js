// Deploys PriceOracle with the action's derived wallet address pinned as
// the signer, and writes the deployed address to .env as PRICE_ORACLE_ADDRESS.
//
// Usage:
//   ACTION_WALLET_ADDRESS=0x... npx hardhat run scripts/deploy.js --network baseSepolia
//
// (also invoked by setup.js via execSync)

const hre = require("hardhat");
const env = require("./_env");

async function main() {
  env.load();
  const signer = process.env.ACTION_WALLET_ADDRESS;
  if (!signer) throw new Error("ACTION_WALLET_ADDRESS env var is required");

  const factory = await hre.ethers.getContractFactory("PriceOracle");
  const oracle = await factory.deploy(signer);
  await oracle.deployed();

  const address = oracle.address;
  console.log("PriceOracle deployed:", address);
  console.log("Signer (action address):", signer);

  env.upsert("PRICE_ORACLE_ADDRESS", address);
  console.log("Wrote PRICE_ORACLE_ADDRESS to .env");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
