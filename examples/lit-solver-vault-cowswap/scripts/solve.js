// Happy path, real CoW settlement. The solver bot asks the cowPolicy action to
// authorize settling the trader's order; the action verifies the order
// signature, builds the entire settle() batch from it, and signs the batch. The
// vault — the allowlisted solver — then forwards the batch to GPv2Settlement,
// paying the receiver from inventory and collecting the trader's sell token.
//
// Usage: node scripts/solve.js   (or: npm run solve)

const { ethers } = require("ethers");
const env = require("./_env");
env.load();
const { CHAIN_ID, VAULT_ABI, MOCK_ERC20_ABI, policyOrderParam, requestSolveAuthorization } = require("./_cow");

async function main() {
  for (const k of [
    "ALCHEMY_BASE_SEPOLIA_URL",
    "COW_VAULT_ADDRESS",
    "COW_ORDER",
    "SOLVER_PRIVATE_KEY",
  ]) {
    if (!process.env[k]) throw new Error(`${k} is required (run setup + order first)`);
  }

  const order = JSON.parse(process.env.COW_ORDER);
  const authDeadline = Math.floor(Date.now() / 1000) + 600;

  console.log("Requesting CoW settlement authorization from Lit...");
  const t0 = Date.now();
  const auth = await requestSolveAuthorization({
    vaultAddress: process.env.COW_VAULT_ADDRESS,
    chainId: CHAIN_ID,
    authDeadline,
    order: policyOrderParam(order, order.owner, order.signature),
    rpcUrl: process.env.ALCHEMY_BASE_SEPOLIA_URL,
  });
  const ms = Date.now() - t0;

  if (!auth || !auth.authorized) {
    console.error("Policy DENIED the settlement:", auth && auth.reason);
    process.exit(2);
  }
  console.log(`Policy authorized in ${ms}ms. Signer: ${auth.signer}`);
  console.log(
    `  settlement pays ${ethers.utils.formatEther(auth.buyAmount)} mWETH -> ${auth.receiver}`
  );
  console.log(
    `  vault spends ${ethers.utils.formatEther(auth.pullAmount)} mWETH from inventory (pullToken ${auth.pullToken})`
  );

  const provider = new ethers.providers.JsonRpcProvider(process.env.ALCHEMY_BASE_SEPOLIA_URL);
  const solver = new ethers.Wallet(process.env.SOLVER_PRIVATE_KEY, provider);
  const vault = new ethers.Contract(process.env.COW_VAULT_ADDRESS, VAULT_ABI, solver);
  const buyToken = new ethers.Contract(auth.buyToken, MOCK_ERC20_ABI, provider);
  const sellToken = new ethers.Contract(auth.sellToken, MOCK_ERC20_ABI, provider);

  const recvBefore = await buyToken.balanceOf(auth.receiver);
  const vaultSellBefore = await sellToken.balanceOf(process.env.COW_VAULT_ADDRESS);

  console.log("Submitting executeSettlement...");
  const tx = await vault.executeSettlement(
    auth.settleCalldata,
    auth.pullToken,
    auth.pullAmount,
    auth.authDeadline,
    auth.signature
  );
  console.log("settle tx:", tx.hash);
  const receipt = await tx.wait();

  // Alchemy is load-balanced and lags read-after-write: the balance right after
  // a mined tx can still read stale on a different node. Poll until the
  // receiver's balance reflects the fill (or give up and print what we see).
  const recvAfter = await waitForDelta(buyToken, auth.receiver, recvBefore);
  const vaultSellAfter = await sellToken.balanceOf(process.env.COW_VAULT_ADDRESS);

  console.log("\n✓ Settlement landed in block", receipt.blockNumber);
  console.log("  receiver mWETH +", ethers.utils.formatEther(recvAfter.sub(recvBefore)));
  console.log(
    "  vault   mUSDC +",
    ethers.utils.formatUnits(vaultSellAfter.sub(vaultSellBefore), 6),
    "(the asset the solver bought)"
  );
}

// Poll a token balance until it differs from `before` (rides out Alchemy's
// read-after-write lag), then return the fresh balance.
async function waitForDelta(token, account, before, tries = 10) {
  for (let i = 0; i < tries; i++) {
    const now = await token.balanceOf(account);
    if (!now.eq(before)) return now;
    await new Promise((r) => setTimeout(r, 1500));
  }
  return token.balanceOf(account);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
