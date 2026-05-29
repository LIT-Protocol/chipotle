// Live policy update — no key rotation, no downtime. The owner adjusts the
// vault's on-chain policy config; the next fill request reads the new values.
//
//   npm run policy -- --max 50       lower the per-fill cap to 50 mUSDC
//   npm run policy -- --kill on      engage the global kill switch
//   npm run policy -- --kill off     release the kill switch
//   npm run policy                   print current policy
//
// After `--max 50`, re-run `npm run fill` (100 mUSDC) and watch it get rejected
// — the same key, the same action, just a tighter policy that took effect on
// the next call.

const { ethers } = require("ethers");
const env = require("./_env");
env.load();

const VAULT_ABI = [
  "function killSwitch() view returns (bool)",
  "function maxFillAmount() view returns (uint256)",
  "function setKillSwitch(bool on)",
  "function setMaxFillAmount(uint256 amount)",
];

function parseArgs() {
  const out = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    out[process.argv[i].replace(/^--/, "")] = process.argv[i + 1];
  }
  return out;
}

async function main() {
  for (const k of ["SOLVER_VAULT_ADDRESS", "DEPLOYER_PRIVATE_KEY", "ALCHEMY_BASE_SEPOLIA_URL"]) {
    if (!process.env[k]) throw new Error(`${k} is required (run \`npm run setup\`)`);
  }
  const args = parseArgs();

  const provider = new ethers.providers.JsonRpcProvider(process.env.ALCHEMY_BASE_SEPOLIA_URL);
  const owner = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, provider);
  const vault = new ethers.Contract(process.env.SOLVER_VAULT_ADDRESS, VAULT_ABI, owner);

  if (args.max !== undefined) {
    const amount = ethers.utils.parseUnits(args.max, 6);
    console.log(`Setting maxFillAmount -> ${args.max} mUSDC...`);
    await (await vault.setMaxFillAmount(amount)).wait();
  }

  if (args.kill !== undefined) {
    const on = args.kill === "on" || args.kill === "true";
    console.log(`Setting killSwitch -> ${on}...`);
    await (await vault.setKillSwitch(on)).wait();
  }

  const [kill, max] = await Promise.all([vault.killSwitch(), vault.maxFillAmount()]);
  console.log("\nCurrent policy:");
  console.log("  killSwitch:    ", kill);
  console.log("  maxFillAmount: ", ethers.utils.formatUnits(max, 6), "mUSDC");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
