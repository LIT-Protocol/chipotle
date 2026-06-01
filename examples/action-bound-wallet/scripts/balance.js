// Show the token + native balance of a user's action-bound wallet.
//
//   npm run balance -- <userIndex>

const { ethers } = require("ethers");
const env = require("./_env");
const { userWallet } = require("./_users");
const { depositAddressFor } = require("./_lit");

async function main() {
  env.load();
  const index = process.argv[2] || "0";

  const { RPC_URL, DEMO_TOKEN_ADDRESS } = process.env;
  if (!DEMO_TOKEN_ADDRESS) throw new Error("DEMO_TOKEN_ADDRESS missing (run `npm run setup`)");

  const provider = new ethers.providers.JsonRpcProvider(RPC_URL);
  const owner = userWallet(index);
  const walletAddress = await depositAddressFor(owner.address);

  const token = new ethers.Contract(
    DEMO_TOKEN_ADDRESS,
    [
      "function balanceOf(address) view returns (uint256)",
      "function decimals() view returns (uint8)",
      "function symbol() view returns (string)",
    ],
    provider
  );
  const [raw, decimals, symbol, gas] = await Promise.all([
    token.balanceOf(walletAddress),
    token.decimals(),
    token.symbol(),
    provider.getBalance(walletAddress),
  ]);

  console.log(`User #${index} action-bound wallet: ${walletAddress}`);
  console.log(`  ${ethers.utils.formatUnits(raw, decimals)} ${symbol}`);
  console.log(`  ${ethers.utils.formatEther(gas)} gas (native)`);
}

main().catch((err) => {
  console.error(err.message);
  process.exit(1);
});
