// Withdraw ERC-20 from a user's action-bound wallet by authorizing it with the
// owner's signature.
//
//   npm run withdraw -- <userIndex> <toAddress> <tokenAmount>
//
// Flow:
//   1. Read the wallet's current nonce + chainId.
//   2. Build the canonical authorization message over the exact withdrawal.
//   3. The OWNER's EOA signs it (this is the only spending authority — the Lit
//      usage key cannot do this).
//   4. The action verifies the signature against its hardcoded OWNER_ADDRESS,
//      then signs the ERC-20 transfer with the wallet's CID-derived key and
//      returns the raw tx.
//   5. We broadcast the raw tx.

const { ethers } = require("ethers");
const env = require("./_env");
const { userWallet } = require("./_users");
const { runUserAction, depositAddressFor } = require("./_lit");
const { withdrawalMessage } = require("./_canonical");

async function main() {
  env.load();
  const index = process.argv[2] || "0";
  const to = process.argv[3];
  const amount = process.argv[4] || "25";
  if (!to) throw new Error("usage: npm run withdraw -- <userIndex> <toAddress> <tokenAmount>");

  const { RPC_URL, DEMO_TOKEN_ADDRESS } = process.env;
  if (!DEMO_TOKEN_ADDRESS) throw new Error("DEMO_TOKEN_ADDRESS missing (run `npm run setup`)");

  const provider = new ethers.providers.JsonRpcProvider(RPC_URL);
  const owner = userWallet(index);
  const walletAddress = await depositAddressFor(owner.address);

  const token = new ethers.Contract(
    DEMO_TOKEN_ADDRESS,
    ["function decimals() view returns (uint8)"],
    provider
  );
  const decimals = await token.decimals();
  const units = ethers.utils.parseUnits(amount, decimals).toString();

  const { chainId } = await provider.getNetwork();
  const nonce = await provider.getTransactionCount(walletAddress, "pending");
  const deadline = Math.floor(Date.now() / 1000) + 600;

  // 2 + 3: build the canonical message and have the OWNER sign it.
  const message = withdrawalMessage({
    wallet: walletAddress,
    chainId,
    token: DEMO_TOKEN_ADDRESS,
    to,
    amount: units,
    nonce,
    deadline,
  });
  const signature = await owner.signMessage(message);

  console.log(`Authorizing withdrawal of ${amount} ABD from ${walletAddress} -> ${to}`);
  console.log(`  signed by owner ${owner.address} (nonce ${nonce}, chain ${chainId})`);

  // 4: the action verifies + signs the transfer.
  const out = await runUserAction(owner.address, {
    action: "withdraw",
    token: DEMO_TOKEN_ADDRESS,
    to,
    amount: units,
    nonce,
    deadline,
    signature,
    chainId,
    rpcUrl: RPC_URL,
  });

  if (!out.ok) {
    console.error(`\n✗ Action refused: ${out.reason}`);
    process.exit(1);
  }

  // 5: broadcast.
  console.log("  action signed the transfer; broadcasting...");
  const sent = await provider.sendTransaction(out.rawTx);
  console.log(`  tx: ${sent.hash}`);
  await sent.wait();
  console.log("\n✓ Withdrawal confirmed. Check balances with: npm run balance --", index);
}

main().catch((err) => {
  console.error(err.message);
  process.exit(1);
});
