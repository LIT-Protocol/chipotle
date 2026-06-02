// Fund a user's action-bound wallet: send it ERC-20 + a little native gas.
//
//   npm run deposit -- <userIndex> [tokenAmount]
//
// The wallet is a plain EOA (its key just happens to live inside the action),
// so to withdraw later it must pay its own gas. We mint the demo token to it
// and top it up with a small amount of native gas from the deployer.

const { ethers } = require("ethers");
const env = require("./_env");
const { userWallet } = require("./_users");
const { depositAddressFor } = require("./_lit");

const GAS_TOPUP_ETH = process.env.GAS_TOPUP_ETH || "0.001";

async function main() {
  env.load();
  const index = process.argv[2] || "0";
  const amount = process.argv[3] || "100";

  const { RPC_URL, DEPLOYER_PRIVATE_KEY, DEMO_TOKEN_ADDRESS } = process.env;
  if (!DEMO_TOKEN_ADDRESS) throw new Error("DEMO_TOKEN_ADDRESS missing (run `npm run setup`)");

  const provider = new ethers.providers.JsonRpcProvider(RPC_URL);
  const deployer = new ethers.Wallet(DEPLOYER_PRIVATE_KEY, provider);

  const owner = userWallet(index);
  const walletAddress = await depositAddressFor(owner.address);
  console.log(`Funding user #${index}'s action-bound wallet: ${walletAddress}`);

  const token = new ethers.Contract(
    DEMO_TOKEN_ADDRESS,
    [
      "function mint(address to, uint256 amount)",
      "function decimals() view returns (uint8)",
    ],
    deployer
  );
  const decimals = await token.decimals();
  const units = ethers.utils.parseUnits(amount, decimals);

  console.log(`  minting ${amount} ABD to the wallet...`);
  await (await token.mint(walletAddress, units)).wait();

  console.log(`  sending ${GAS_TOPUP_ETH} gas to the wallet...`);
  await (
    await deployer.sendTransaction({
      to: walletAddress,
      value: ethers.utils.parseEther(GAS_TOPUP_ETH),
    })
  ).wait();

  console.log("\n✓ Funded. Check it with: npm run balance --", index);
}

main().catch((err) => {
  console.error(err.message);
  process.exit(1);
});
