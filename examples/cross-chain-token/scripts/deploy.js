// Deploys a single BridgeToken on the network passed in via `--network`,
// pinning the action's derived wallet address as the bridgeOracle. Writes
// the resulting address to .env under a per-network key
// (BRIDGE_TOKEN_BASE_SEPOLIA / BRIDGE_TOKEN_ARB_SEPOLIA) so setup.js can
// wire both deployments together afterwards.
//
// Usage:
//   ACTION_WALLET_ADDRESS=0x... npx hardhat run scripts/deploy.js --network baseSepolia
//   ACTION_WALLET_ADDRESS=0x... npx hardhat run scripts/deploy.js --network arbitrumSepolia
//
// (also invoked twice by setup.js via execSync)

const hre = require("hardhat");
const env = require("./_env");

// Map Hardhat network name -> .env key that holds the resulting address.
// Keeps deploy outputs from clobbering each other when setup.js runs both.
const ENV_KEY_BY_NETWORK = {
  baseSepolia: "BRIDGE_TOKEN_BASE_SEPOLIA",
  arbitrumSepolia: "BRIDGE_TOKEN_ARB_SEPOLIA",
};

async function main() {
  env.load();
  const oracle = process.env.ACTION_WALLET_ADDRESS;
  if (!oracle) throw new Error("ACTION_WALLET_ADDRESS env var is required");

  const networkName = hre.network.name;
  const envKey = ENV_KEY_BY_NETWORK[networkName];
  if (!envKey) {
    throw new Error(
      `unknown network ${networkName} — add it to ENV_KEY_BY_NETWORK in deploy.js`
    );
  }

  const name = process.env.TOKEN_NAME || "Bridge Coin";
  const symbol = process.env.TOKEN_SYMBOL || "BRDG";

  // Only mint initial supply on the configured "home" chain so re-running
  // the example doesn't double-mint. Defaults to Base Sepolia. Tokens get
  // to the other chain by burning + minting through the bridge — that's
  // the whole point.
  const homeNetwork = process.env.INITIAL_SUPPLY_NETWORK || "baseSepolia";
  const initialSupplyRaw = process.env.INITIAL_SUPPLY || "1000000";
  const initialSupply =
    networkName === homeNetwork
      ? hre.ethers.utils.parseUnits(initialSupplyRaw, 18)
      : hre.ethers.BigNumber.from(0);

  const factory = await hre.ethers.getContractFactory("BridgeToken");
  const token = await factory.deploy(name, symbol, initialSupply, oracle);
  await token.deployed();

  const address = token.address;
  console.log(`BridgeToken deployed on ${networkName}:`, address);
  console.log("  bridgeOracle (action address):", oracle);
  console.log("  initial supply minted to deployer:", initialSupply.toString());

  env.upsert(envKey, address);
  console.log(`Wrote ${envKey} to .env`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
