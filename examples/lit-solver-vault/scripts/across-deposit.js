// Create a real Across intent on the origin chain (Sepolia). We play the
// depositor here so the demo is self-contained — wrap a little ETH to WETH,
// approve the SpokePool, and call depositV3 with no exclusive relayer so our
// vault can fill it immediately.
//
// Writes ACROSS_DEPOSIT_ID + ACROSS_DEPOSIT_BLOCK to .env for the fill step.
//
// Usage: node scripts/across-deposit.js   (or: npm run across:deposit)

const { ethers } = require("ethers");
const env = require("./_env");
env.load();
const {
  ORIGIN_SPOKE,
  ORIGIN_WETH,
  DEST_WETH,
  DEST_CHAIN_ID,
  WETH_ABI,
  SPOKE_DEPOSIT_ABI,
} = require("./_across");

const ETH = (v) => ethers.utils.parseEther(String(v));

async function main() {
  for (const k of ["DEPLOYER_PRIVATE_KEY", "ALCHEMY_ETH_SEPOLIA_URL"]) {
    if (!process.env[k]) throw new Error(`${k} is required`);
  }

  const provider = new ethers.providers.JsonRpcProvider(process.env.ALCHEMY_ETH_SEPOLIA_URL);
  const depositor = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, provider);
  const recipient = process.env.ORDER_RECIPIENT || depositor.address;

  const inputAmount = ETH(process.env.ACROSS_DEPOSIT_ETH || "0.001");
  // 1% relayer fee — the relayer pays out less than the deposit locks.
  const outputAmount = inputAmount.mul(99).div(100);

  const weth = new ethers.Contract(ORIGIN_WETH, WETH_ABI, depositor);
  const spoke = new ethers.Contract(ORIGIN_SPOKE, SPOKE_DEPOSIT_ABI, depositor);

  // Ensure the depositor holds enough WETH; wrap ETH if not.
  const bal = await weth.balanceOf(depositor.address);
  if (bal.lt(inputAmount)) {
    console.log(`Wrapping ${ethers.utils.formatEther(inputAmount.sub(bal))} ETH -> WETH...`);
    await (await weth.deposit({ value: inputAmount.sub(bal) })).wait();
  }
  const allowance = await weth.allowance(depositor.address, ORIGIN_SPOKE);
  if (allowance.lt(inputAmount)) {
    console.log("Approving SpokePool to pull WETH...");
    await (await weth.approve(ORIGIN_SPOKE, ethers.constants.MaxUint256)).wait();
  }

  // Make OUR vault the exclusive relayer for a window, so a public Across
  // testnet relayer can't snipe the deposit before our vault fills it. (An
  // open deposit — exclusiveRelayer = 0 — gets filled by whoever's fastest,
  // which on a live testnet is usually someone else.) This is also the more
  // realistic solver setup: solvers routinely take exclusive fills.
  const exclusiveRelayer = process.env.ACROSS_VAULT_ADDRESS;
  if (!exclusiveRelayer) {
    throw new Error("ACROSS_VAULT_ADDRESS is required (run `npm run across:setup` first)");
  }

  // Deadlines relative to the chain's clock and the SpokePool's buffers.
  const block = await provider.getBlock("latest");
  const now = block.timestamp;
  const fillBuffer = await spoke.fillDeadlineBuffer();
  const fillDeadline = now + Math.min(Number(fillBuffer), 3600);
  const exclusivityDeadline = now + 1800; // vault-only for 30 min (absolute ts)
  const quoteTimestamp = now; // within depositQuoteTimeBuffer by construction

  console.log(`Depositing ${ethers.utils.formatEther(inputAmount)} WETH on Sepolia`);
  console.log(`  -> ${ethers.utils.formatEther(outputAmount)} WETH to ${recipient} on chain ${DEST_CHAIN_ID}`);
  console.log(`  exclusive relayer (our vault): ${exclusiveRelayer}`);

  const tx = await spoke.depositV3(
    depositor.address,
    recipient,
    ORIGIN_WETH,
    DEST_WETH,
    inputAmount,
    outputAmount,
    DEST_CHAIN_ID,
    exclusiveRelayer,
    quoteTimestamp,
    fillDeadline,
    exclusivityDeadline,
    "0x"
  );
  console.log("deposit tx:", tx.hash);
  const receipt = await tx.wait();

  // Pull depositId out of the FundsDeposited event.
  const iface = new ethers.utils.Interface(SPOKE_DEPOSIT_ABI);
  let depositId;
  for (const log of receipt.logs) {
    if (log.address.toLowerCase() !== ORIGIN_SPOKE.toLowerCase()) continue;
    try {
      const parsed = iface.parseLog(log);
      if (parsed.name === "FundsDeposited") {
        depositId = parsed.args.depositId;
        break;
      }
    } catch {
      /* not our event */
    }
  }
  if (depositId === undefined) throw new Error("could not find FundsDeposited in receipt");

  env.upsert("ACROSS_DEPOSIT_ID", String(depositId));
  env.upsert("ACROSS_DEPOSIT_BLOCK", String(receipt.blockNumber));
  console.log(`\n✓ Deposit ${depositId} created in block ${receipt.blockNumber}.`);
  console.log("  Next: npm run across:fill");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
