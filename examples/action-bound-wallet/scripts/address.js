// Show a user's action-bound deposit wallet address.
//
//   npm run address -- <userIndex>
//
// The address is derived from the user's action CID, which depends on the
// user's address being stamped into the code. Run it for two different users
// and you get two different wallets — proof that the binding is per-user.

const env = require("./_env");
const { userWallet } = require("./_users");
const { depositAddressFor } = require("./_lit");

async function main() {
  env.load();
  const index = process.argv[2] || "0";

  const owner = userWallet(index);
  console.log(`User #${index}`);
  console.log("  owner EOA (bound into the action):", owner.address);

  const walletAddress = await depositAddressFor(owner.address);
  console.log("  action-bound wallet (deposit here):", walletAddress);
  console.log("\nAnyone can read this address; only the owner above can spend from it.");
}

main().catch((err) => {
  console.error(err.message);
  process.exit(1);
});
