// Deploys PriceConsumer on the network passed via `--network`, pinning the
// action's derived wallet as the immutable `updater`. Writes the address to
// .env. Invoked by setup.js after the relayer wallet is known.

const hre = require("hardhat");
const env = require("./_env");

const ENV_KEY_BY_NETWORK = {
  baseSepolia: "PRICE_CONSUMER_BASE_SEPOLIA",
};

async function main() {
  env.load();
  const updater = process.env.ACTION_WALLET_ADDRESS;
  if (!updater) throw new Error("ACTION_WALLET_ADDRESS env var is required");

  const networkName = hre.network.name;
  const envKey = ENV_KEY_BY_NETWORK[networkName];
  if (!envKey) throw new Error(`unknown network ${networkName} — add it to ENV_KEY_BY_NETWORK`);

  const factory = await hre.ethers.getContractFactory("PriceConsumer");
  const consumer = await factory.deploy(updater);
  await consumer.deployed();

  console.log(`PriceConsumer deployed on ${networkName}:`, consumer.address);
  console.log("  updater (relayer/action wallet):", updater);

  env.upsert(envKey, consumer.address);
  console.log(`Wrote ${envKey} to .env`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
