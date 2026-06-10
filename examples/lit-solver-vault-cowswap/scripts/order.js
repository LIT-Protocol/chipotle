// Play the trader: create a real, EIP-712-signed CoW order (the "intent"), and
// approve the settlement's VaultRelayer to pull the sell token — exactly what a
// user does when they place an order on CoW. We mint the trader some sell-token
// so the demo is self-contained.
//
// Writes COW_ORDER (the signed order JSON) to .env for the solve / attack steps.
//
// Usage: node scripts/order.js   (or: npm run order)

const { ethers } = require("ethers");
const env = require("./_env");
env.load();
const { MOCK_ERC20_ABI, buildOrder, signOrder } = require("./_cow");

async function main() {
  for (const k of [
    "ALCHEMY_BASE_SEPOLIA_URL",
    "COW_SELL_TOKEN",
    "COW_BUY_TOKEN",
    "COW_SETTLEMENT_ADDRESS",
    "COW_VAULT_RELAYER",
  ]) {
    if (!process.env[k]) throw new Error(`${k} is required (run \`npm run setup\` first)`);
  }

  const provider = new ethers.providers.JsonRpcProvider(process.env.ALCHEMY_BASE_SEPOLIA_URL);
  // The trader is a separate party from the solver. Defaults to the deployer
  // key for a self-contained demo; set TRADER_PRIVATE_KEY for a distinct one.
  const trader = new ethers.Wallet(
    process.env.TRADER_PRIVATE_KEY || process.env.DEPLOYER_PRIVATE_KEY,
    provider
  );
  const receiver = process.env.COW_ORDER_RECEIVER || trader.address;

  const sell = new ethers.Contract(process.env.COW_SELL_TOKEN, MOCK_ERC20_ABI, trader);
  const sellDecimals = await sell.decimals();
  const sellAmount = ethers.utils.parseUnits(process.env.COW_SELL_AMOUNT || "100", sellDecimals);
  const buyAmount = ethers.utils.parseEther(process.env.COW_BUY_AMOUNT || "0.03"); // mWETH (18dp)

  // Ensure the trader holds enough sell-token; mint if not (faucet token).
  const bal = await sell.balanceOf(trader.address);
  if (bal.lt(sellAmount)) {
    console.log(`Minting ${ethers.utils.formatUnits(sellAmount, sellDecimals)} mUSDC to trader...`);
    await (await sell.mint(trader.address, sellAmount)).wait();
  }

  // CoW pulls the sell token via the VaultRelayer — the trader approves it.
  const relayer = process.env.COW_VAULT_RELAYER;
  const allowance = await sell.allowance(trader.address, relayer);
  if (allowance.lt(sellAmount)) {
    console.log("Approving the VaultRelayer to pull the sell token...");
    await (await sell.approve(relayer, ethers.constants.MaxUint256)).wait();
  }

  const block = await provider.getBlock("latest");
  const validTo = block.timestamp + 3600; // 1h

  const order = buildOrder({
    sellToken: process.env.COW_SELL_TOKEN,
    buyToken: process.env.COW_BUY_TOKEN,
    receiver,
    sellAmount,
    buyAmount,
    validTo,
  });
  const signature = await signOrder(trader, process.env.COW_SETTLEMENT_ADDRESS, order);

  const record = { ...order, owner: trader.address, signature };
  env.upsert("COW_ORDER", JSON.stringify(record));

  console.log("\n✓ Order signed by trader", trader.address);
  console.log(`  sells ${ethers.utils.formatUnits(sellAmount, sellDecimals)} mUSDC`);
  console.log(`  for   ${ethers.utils.formatEther(buyAmount)} mWETH -> ${receiver}`);
  console.log(`  validTo ${validTo}`);
  console.log("\nNext: npm run solve");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
