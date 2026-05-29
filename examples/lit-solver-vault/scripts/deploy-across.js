// Deploys + funds the AcrossSolverVault on the destination chain (Base Sepolia):
//   1. AcrossSolverVault (pins SpokePool, policy signer, owner, cold wallet, cap)
//   2. wrap ETH -> WETH and move it into the vault as fill inventory
//   3. allowlist the origin chain (Sepolia) on the vault
//
// Invoked by setup-across.js via execSync once ACROSS_POLICY_SIGNER_ADDRESS
// has been derived.

const hre = require("hardhat");
const env = require("./_env");
const { DEST_SPOKE, DEST_WETH, ORIGIN_CHAIN_ID } = require("./_across");

const ETH = (v) => hre.ethers.utils.parseEther(String(v));

// Poll balanceOf until it reflects at least `target`, to ride out Alchemy's
// read-after-write lag between a mined tx and the next call's gas estimate.
async function waitForBalance(token, who, target, tries = 15) {
  for (let i = 0; i < tries; i++) {
    if ((await token.balanceOf(who)).gte(target)) return;
    await new Promise((r) => setTimeout(r, 2000));
  }
  throw new Error("timed out waiting for wrapped WETH balance to settle");
}

async function main() {
  env.load();

  const policySigner = process.env.ACROSS_POLICY_SIGNER_ADDRESS;
  if (!policySigner) {
    throw new Error("ACROSS_POLICY_SIGNER_ADDRESS is required (run `npm run across:setup`)");
  }

  const [deployer] = await hre.ethers.getSigners();
  const owner = deployer.address;
  const coldWallet = process.env.COLD_WALLET || owner;
  const maxFill = ETH(process.env.ACROSS_MAX_FILL_ETH || "0.005");
  const inventory = ETH(process.env.ACROSS_INVENTORY_ETH || "0.01");

  console.log("Deployer / owner:", owner);
  console.log("Cold wallet:     ", coldWallet);
  console.log("Policy signer:   ", policySigner);
  console.log("SpokePool:       ", DEST_SPOKE);

  const Vault = await hre.ethers.getContractFactory("AcrossSolverVault");
  const vault = await Vault.deploy(DEST_SPOKE, policySigner, owner, coldWallet, maxFill);
  await vault.deployed();
  console.log("AcrossSolverVault:", vault.address);

  // Wrap ETH -> WETH, then fund the vault with inventory.
  const weth = new hre.ethers.Contract(
    DEST_WETH,
    [
      "function deposit() payable",
      "function transfer(address,uint256) returns (bool)",
      "function balanceOf(address) view returns (uint256)",
    ],
    deployer
  );
  const have = await weth.balanceOf(owner);
  if (have.lt(inventory)) {
    console.log(`Wrapping ${hre.ethers.utils.formatEther(inventory.sub(have))} ETH -> WETH...`);
    await (await weth.deposit({ value: inventory.sub(have) })).wait();
    // Alchemy is load-balanced and read-after-write lags: a freshly-mined
    // balance may not be visible to the node that estimates the next tx's gas,
    // making transfer revert with a phantom "insufficient balance." Poll until
    // the wrapped balance is visible before transferring.
    await waitForBalance(weth, owner, inventory);
  }
  await (await weth.transfer(vault.address, inventory)).wait();
  console.log(`Funded vault with ${hre.ethers.utils.formatEther(inventory)} WETH`);

  await (await vault.setAllowedOriginChain(ORIGIN_CHAIN_ID, true)).wait();
  console.log(`Allowlisted origin chain ${ORIGIN_CHAIN_ID} on vault`);

  env.upsert("ACROSS_VAULT_ADDRESS", vault.address);
  env.upsert("COLD_WALLET", coldWallet);
  console.log("Wrote ACROSS_VAULT_ADDRESS to .env");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
