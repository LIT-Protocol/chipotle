// Deploys ReleaseRegistry on the network passed via `--network`, pinning the
// action's derived wallet address as the immutable `attester`. Writes the
// deployed address to .env. Invoked by setup.js after the action wallet is
// known.
//
// Usage:
//   ACTION_WALLET_ADDRESS=0x... npx hardhat run scripts/deploy.js --network baseSepolia

const hre = require("hardhat");
const env = require("./_env");

const ENV_KEY_BY_NETWORK = {
  baseSepolia: "RELEASE_REGISTRY_BASE_SEPOLIA",
};

async function main() {
  env.load();
  const attester = process.env.ACTION_WALLET_ADDRESS;
  if (!attester) throw new Error("ACTION_WALLET_ADDRESS env var is required");

  const networkName = hre.network.name;
  const envKey = ENV_KEY_BY_NETWORK[networkName];
  if (!envKey) {
    throw new Error(`unknown network ${networkName} — add it to ENV_KEY_BY_NETWORK`);
  }

  const factory = await hre.ethers.getContractFactory("ReleaseRegistry");
  const registry = await factory.deploy(attester);
  await registry.deployed();

  console.log(`ReleaseRegistry deployed on ${networkName}:`, registry.address);
  console.log("  attester (action wallet):", attester);

  env.upsert(envKey, registry.address);
  console.log(`Wrote ${envKey} to .env`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
