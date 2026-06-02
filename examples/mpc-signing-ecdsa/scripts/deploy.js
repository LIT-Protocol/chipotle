// Deploys MpcVault with the MPC-derived address (from keygen) pinned as
// the signer, and writes the deployed address to .env as VAULT_ADDRESS.
//
// Usage:
//   npx hardhat run scripts/deploy.js --network baseSepolia
//   (or: npm run deploy:baseSepolia)

const hre = require("hardhat");
const env = require("./_env");

async function main() {
  env.load();
  const signer = process.env.VAULT_SIGNER_ADDRESS;
  if (!signer) throw new Error("VAULT_SIGNER_ADDRESS is required (run `npm run keygen` first)");

  const factory = await hre.ethers.getContractFactory("MpcVault");
  const vault = await factory.deploy(signer);
  await vault.deployed();

  console.log("MpcVault deployed:", vault.address);
  console.log("Signer (MPC address):  ", signer);

  env.upsert("VAULT_ADDRESS", vault.address);
  console.log("Wrote VAULT_ADDRESS to .env");
  console.log("\nFund the vault with a little native gas-token, then:");
  console.log("  npm run sign -- --to 0xRecipient --value 0.001");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
