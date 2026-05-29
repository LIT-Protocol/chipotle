// Emergency exit. Pretend Lit is unreachable: no fills can be authorized. The
// owner sweeps the vault's inventory to the pinned cold wallet with a single
// call that needs no Lit involvement at all.
//
// This is the liveness guarantee in the pitch: a Lit outage stops you from
// *earning*, it never traps your *inventory*. And because the destination is
// pinned (changing it is timelocked), even a compromised owner key can only
// push funds to the cold wallet you already approved.
//
// Usage: node scripts/exit.js   (or: npm run exit)

const { ethers } = require("ethers");
const env = require("./_env");
env.load();

async function main() {
  for (const k of [
    "SOLVER_VAULT_ADDRESS",
    "MOCK_USDC_ADDRESS",
    "DEPLOYER_PRIVATE_KEY",
    "ALCHEMY_BASE_SEPOLIA_URL",
  ]) {
    if (!process.env[k]) throw new Error(`${k} is required (run \`npm run setup\`)`);
  }

  const provider = new ethers.providers.JsonRpcProvider(process.env.ALCHEMY_BASE_SEPOLIA_URL);
  const owner = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, provider);

  const vault = new ethers.Contract(
    process.env.SOLVER_VAULT_ADDRESS,
    ["function coldWallet() view returns (address)", "function exit(address token)"],
    owner
  );
  const usdc = new ethers.Contract(
    process.env.MOCK_USDC_ADDRESS,
    ["function balanceOf(address) view returns (uint256)"],
    provider
  );

  const coldWallet = await vault.coldWallet();
  const before = await usdc.balanceOf(process.env.SOLVER_VAULT_ADDRESS);
  console.log("Cold wallet:        ", coldWallet);
  console.log("Vault balance:      ", ethers.utils.formatUnits(before, 6), "mUSDC");

  console.log("Sweeping vault -> cold wallet (no Lit involved)...");
  const tx = await vault.exit(process.env.MOCK_USDC_ADDRESS);
  console.log("tx:", tx.hash);
  await tx.wait();

  const after = await usdc.balanceOf(process.env.SOLVER_VAULT_ADDRESS);
  const cold = await usdc.balanceOf(coldWallet);
  console.log("\n✓ Exit complete.");
  console.log("  Vault balance:    ", ethers.utils.formatUnits(after, 6), "mUSDC");
  console.log("  Cold wallet bal:  ", ethers.utils.formatUnits(cold, 6), "mUSDC");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
