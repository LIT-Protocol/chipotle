// Deploys CompliantToken with the action's derived wallet address pinned
// as the compliance oracle.
//
// Note: the oracle address is *not* the decrypt PKP. The contract trusts
// signatures from the action's IPFS-CID-derived key
// (Lit.Actions.getLitActionPrivateKey inside the action). The PKP is only
// involved in the encryption side of the flow, and is irrelevant to the
// contract.
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

  const name = process.env.TOKEN_NAME || "Compliant USD";
  const symbol = process.env.TOKEN_SYMBOL || "cUSD";
  const initialSupply = hre.ethers.utils.parseUnits(
    process.env.INITIAL_SUPPLY || "1000000",
    18
  );

  const factory = await hre.ethers.getContractFactory("CompliantToken");
  const token = await factory.deploy(name, symbol, initialSupply, oracle);
  await token.deployed();

  const address = token.address;
  console.log("CompliantToken deployed:", address);
  console.log("Compliance oracle (action address):", oracle);
  console.log("Initial supply minted to deployer:", initialSupply.toString());

  env.upsert("COMPLIANT_TOKEN_ADDRESS", address);
  console.log("Wrote COMPLIANT_TOKEN_ADDRESS to .env");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
