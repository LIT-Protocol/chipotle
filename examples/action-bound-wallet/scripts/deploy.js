// Deploys the DemoToken ERC-20 used by the example and records its address.
//
// Usage:
//   npx hardhat run scripts/deploy.js --network baseSepolia
// (also invoked by setup.js via execSync)

const hre = require("hardhat");
const env = require("./_env");

async function main() {
  env.load();

  const [deployer] = await hre.ethers.getSigners();
  console.log("Deployer:", deployer.address);

  // Mint 1,000,000 ABD to the deployer; deposit.js hands these out to wallets.
  const DemoToken = await hre.ethers.getContractFactory("DemoToken");
  const token = await DemoToken.deploy(hre.ethers.utils.parseUnits("1000000", 18));
  await token.deployed();
  console.log("DemoToken:", token.address);

  env.upsert("DEMO_TOKEN_ADDRESS", token.address);
  console.log("Wrote DEMO_TOKEN_ADDRESS to .env");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
