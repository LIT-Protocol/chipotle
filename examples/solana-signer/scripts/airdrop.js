// Fund the action's Solana wallet with devnet SOL so it can pay fees and have
// something to transfer.
//
// Usage:
//   npm run airdrop            # 1 SOL to the action wallet
//   npm run airdrop -- 2       # 2 SOL
//
// Devnet faucet airdrops are rate-limited and occasionally flaky; if this
// fails, use the web faucet at https://faucet.solana.com (paste the address
// from `npm run address`) or the CLI: `solana airdrop 1 <address> --url devnet`.

const {
  Connection,
  PublicKey,
  LAMPORTS_PER_SOL,
  clusterApiUrl,
} = require("@solana/web3.js");
const env = require("./_env");
const { runAction } = require("./_lit");
env.load();

async function main() {
  const sol = Number(process.argv[2] || "1");
  const rpcUrl = process.env.SOLANA_RPC_URL || clusterApiUrl("devnet");

  // Prefer the address cached by setup; fall back to deriving it live.
  const address = process.env.SOLANA_ADDRESS || (await runAction({ action: "address" })).address;
  const pubkey = new PublicKey(address);

  const connection = new Connection(rpcUrl, "confirmed");
  console.log(`Requesting ${sol} SOL airdrop to ${address} on ${rpcUrl} ...`);
  const sig = await connection.requestAirdrop(pubkey, sol * LAMPORTS_PER_SOL);
  await connection.confirmTransaction(sig, "confirmed");

  const balance = await connection.getBalance(pubkey);
  console.log(`airdrop tx: ${sig}`);
  console.log(`balance:    ${balance / LAMPORTS_PER_SOL} SOL`);
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
