// Deploys MockUSDC (testnet reserve asset) and PrivUSD, pinning the ledger
// action's CID-derived wallet address as the contract's sole authority.
//
// The pinned address is NOT the ledger PKP. The contract trusts signatures
// from the action's IPFS-CID-derived key (Lit.Actions.getLitActionPrivateKey
// inside action/ledger.js). The PKP is only the encrypt/decrypt key and is
// invisible to the contract.
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

  const [deployer] = await hre.ethers.getSigners();

  // 1. Reserve asset. On a real deployment this is the canonical USDC address;
  //    on testnet we deploy a mintable mock.
  const usdcFactory = await hre.ethers.getContractFactory("MockUSDC");
  const usdc = await usdcFactory.deploy();
  await usdc.deployed();
  console.log("MockUSDC deployed:", usdc.address);

  // Fund the deployer with 1,000,000 USDC (6 decimals) so the demo can mint.
  // Explicit gasLimit: the public Base Sepolia RPC sometimes returns a bad
  // gas estimate for a just-deployed contract (the deploy tx may not have
  // propagated to the node serving eth_estimateGas yet), causing an
  // out-of-gas revert. A fixed limit sidesteps the flaky estimate.
  const seed = hre.ethers.utils.parseUnits("1000000", 6);
  await (await usdc.mint(deployer.address, seed, { gasLimit: 120000 })).wait();
  console.log("Minted 1,000,000 mock USDC to deployer:", deployer.address);

  // 2. PrivUSD, backed by the reserve, authorized by the action signer.
  const privFactory = await hre.ethers.getContractFactory("PrivUSD");
  const priv = await privFactory.deploy(usdc.address, oracle);
  await priv.deployed();
  console.log("PrivUSD deployed:", priv.address);
  console.log("Ledger oracle (action address):", oracle);

  env.upsert("MOCK_USDC_ADDRESS", usdc.address);
  env.upsert("PRIVUSD_ADDRESS", priv.address);
  console.log("Wrote MOCK_USDC_ADDRESS and PRIVUSD_ADDRESS to .env");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
