// Happy path, real Across fill. The relayer bot asks the acrossPolicy action
// to authorize filling the deposit; the action reads the deposit on-chain,
// reconstructs the relay, and signs it. The vault then calls the SpokePool's
// fillV3Relay, paying the recipient from inventory.
//
// Usage: node scripts/across-fill.js   (or: npm run across:fill)

const { ethers } = require("ethers");
const env = require("./_env");
env.load();
const {
  DEST_WETH,
  VAULT_FILL_ABI,
  requestAcrossAuthorization,
  authParams,
  relayTuple,
} = require("./_across");

async function main() {
  for (const k of [
    "ACROSS_VAULT_ADDRESS",
    "ACROSS_DEPOSIT_ID",
    "SOLVER_PRIVATE_KEY",
    "ALCHEMY_BASE_SEPOLIA_URL",
    "ALCHEMY_ETH_SEPOLIA_URL",
  ]) {
    if (!process.env[k]) throw new Error(`${k} is required (run the deposit + setup steps)`);
  }

  console.log(`Requesting Across fill authorization for deposit ${process.env.ACROSS_DEPOSIT_ID}...`);
  const t0 = Date.now();
  const auth = await requestAcrossAuthorization(authParams());
  const ms = Date.now() - t0;

  if (!auth || !auth.authorized) {
    console.error("Policy DENIED the fill:", auth && auth.reason);
    process.exit(2);
  }
  console.log(`Policy authorized in ${ms}ms. Signer: ${auth.signer}`);
  console.log(`  relay pays ${ethers.utils.formatEther(auth.relayData.outputAmount)} WETH -> ${auth.relayData.recipient}`);

  const provider = new ethers.providers.JsonRpcProvider(process.env.ALCHEMY_BASE_SEPOLIA_URL);
  const solver = new ethers.Wallet(process.env.SOLVER_PRIVATE_KEY, provider);
  const vault = new ethers.Contract(process.env.ACROSS_VAULT_ADDRESS, VAULT_FILL_ABI, solver);
  const weth = new ethers.Contract(
    DEST_WETH,
    ["function balanceOf(address) view returns (uint256)"],
    provider
  );

  const before = await weth.balanceOf(auth.relayData.recipient);
  console.log("Submitting executeAcrossFill...");
  const tx = await vault.executeAcrossFill(
    relayTuple(auth.relayData),
    auth.repaymentChainId,
    auth.authDeadline,
    auth.signature
  );
  console.log("fill tx:", tx.hash);
  const receipt = await tx.wait();
  const after = await weth.balanceOf(auth.relayData.recipient);

  console.log("\n✓ Fill landed in block", receipt.blockNumber);
  console.log("  recipient WETH +", ethers.utils.formatEther(after.sub(before)));
  console.log("  (testnet: no reimbursement bundle runs, so the relayer isn't repaid — fine for the demo)");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
