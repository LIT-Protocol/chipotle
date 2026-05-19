// Deploys ConsensusOracle with the action's derived wallet address pinned
// as the signer.
//
// Note: the signer address is *not* the decrypt PKP. The registry trusts
// signatures from the action's IPFS-CID-derived key
// (Lit.Actions.getLitActionPrivateKey inside the action). The PKP is only
// involved in the encryption side of the flow, and is irrelevant to the
// registry.
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

  const factory = await hre.ethers.getContractFactory("ConsensusOracle");
  const oracle = await factory.deploy(signer);
  await oracle.waitForDeployment();

  const address = await oracle.getAddress();
  console.log("ConsensusOracle deployed:", address);
  console.log("Signer (action address):", signer);

  env.upsert("CONSENSUS_ORACLE_ADDRESS", address);
  console.log("Wrote CONSENSUS_ORACLE_ADDRESS to .env");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
