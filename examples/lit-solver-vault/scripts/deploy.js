// Deploys the demo stack and wires it together:
//   1. MockUSDC               (the solver's inventory token)
//   2. MockSettlement         (stand-in intent/order book)
//   3. SolverVault            (pins POLICY_SIGNER_ADDRESS as the fill signer,
//                              DEPLOYER as owner, COLD_WALLET as exit dest)
//   4. fund the vault with inventory
//   5. allowlist the settlement contract on the vault
//   6. post one sample order so the happy-path fill has something to fill
//
// Usage:
//   ACTION derived → npx hardhat run scripts/deploy.js --network baseSepolia
// (also invoked by setup.js via execSync once it has POLICY_SIGNER_ADDRESS)

const hre = require("hardhat");
const env = require("./_env");

// 6-decimal helpers for MockUSDC.
const USDC = (whole) => hre.ethers.utils.parseUnits(String(whole), 6);

async function main() {
  env.load();

  const policySigner = process.env.POLICY_SIGNER_ADDRESS;
  if (!policySigner) {
    throw new Error("POLICY_SIGNER_ADDRESS is required (run `npm run setup`, which derives it)");
  }

  const [deployer] = await hre.ethers.getSigners();
  const owner = deployer.address;
  const coldWallet = process.env.COLD_WALLET || owner;
  const orderRecipient = process.env.ORDER_RECIPIENT || owner;
  const maxFillUsdc = process.env.MAX_FILL_USDC || "1000";

  console.log("Deployer / owner:", owner);
  console.log("Cold wallet:     ", coldWallet);
  console.log("Policy signer:   ", policySigner);

  // 1. MockUSDC — mint 1,000,000 to the deployer.
  const MockUSDC = await hre.ethers.getContractFactory("MockUSDC");
  const usdc = await MockUSDC.deploy(USDC(1_000_000));
  await usdc.deployed();
  console.log("MockUSDC:        ", usdc.address);

  // 2. MockSettlement.
  const MockSettlement = await hre.ethers.getContractFactory("MockSettlement");
  const settlement = await MockSettlement.deploy();
  await settlement.deployed();
  console.log("MockSettlement:  ", settlement.address);

  // 3. SolverVault.
  const SolverVault = await hre.ethers.getContractFactory("SolverVault");
  const vault = await SolverVault.deploy(
    policySigner,
    owner,
    coldWallet,
    USDC(maxFillUsdc)
  );
  await vault.deployed();
  console.log("SolverVault:     ", vault.address);

  // 4. Fund the vault with 100,000 mUSDC of inventory.
  await (await usdc.transfer(vault.address, USDC(100_000))).wait();
  console.log("Funded vault with 100,000 mUSDC");

  // 5. Allowlist the settlement contract.
  await (await vault.setAllowedSettlement(settlement.address, true)).wait();
  console.log("Allowlisted settlement on vault");

  // 6. Post a sample order: pay `orderRecipient` 100 mUSDC.
  const sampleDepositId = hre.ethers.utils.id("lit-solver-vault:sample-order-1");
  await (
    await settlement.postOrder(sampleDepositId, orderRecipient, usdc.address, USDC(100))
  ).wait();
  console.log("Posted sample order:", sampleDepositId, "->", orderRecipient, "100 mUSDC");

  env.upsert("MOCK_USDC_ADDRESS", usdc.address);
  env.upsert("MOCK_SETTLEMENT_ADDRESS", settlement.address);
  env.upsert("SOLVER_VAULT_ADDRESS", vault.address);
  env.upsert("SAMPLE_DEPOSIT_ID", sampleDepositId);
  env.upsert("COLD_WALLET", coldWallet);
  env.upsert("ORDER_RECIPIENT", orderRecipient);
  console.log("Wrote contract addresses to .env");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
