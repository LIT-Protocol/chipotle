// Send a little SOL to the MPC address so it can pay rent + transaction fees.
//
//   npm run fund                 # transfer 0.05 SOL from FUNDER_SECRET_KEY (or,
//                                # on devnet with no funder set, request an airdrop)
//   npm run fund -- --sol 0.1    # custom amount
//
// The MPC address is a normal Solana account; this just seeds it. On devnet you
// can also `solana airdrop 1 <address>` yourself instead of running this.

const env = require("./_env");
env.load();

const {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  LAMPORTS_PER_SOL,
} = require("@solana/web3.js");
const bs58 = require("bs58");
const store = require("../client/store");

const { SOLANA_RPC_URL = "https://api.devnet.solana.com", FUNDER_SECRET_KEY } = process.env;

function parseSol() {
  const i = process.argv.indexOf("--sol");
  return i !== -1 ? parseFloat(process.argv[i + 1]) : 0.05;
}

async function main() {
  const hot = store.load();
  const address = new PublicKey(Buffer.from(hot.solanaPubkey, "base64"));
  const sol = parseSol();
  const lamports = Math.round(sol * LAMPORTS_PER_SOL);
  const conn = new Connection(SOLANA_RPC_URL, "confirmed");

  console.log(`MPC address: ${address.toBase58()}`);
  console.log(`Cluster:     ${SOLANA_RPC_URL}`);
  console.log(`Balance:     ${(await conn.getBalance(address)) / LAMPORTS_PER_SOL} SOL\n`);

  if (FUNDER_SECRET_KEY) {
    const funder = Keypair.fromSecretKey(bs58.decode(FUNDER_SECRET_KEY));
    console.log(`Transferring ${sol} SOL from ${funder.publicKey.toBase58()}...`);
    const tx = new Transaction().add(
      SystemProgram.transfer({ fromPubkey: funder.publicKey, toPubkey: address, lamports })
    );
    const sig = await conn.sendTransaction(tx, [funder]);
    await conn.confirmTransaction(sig, "confirmed");
    console.log("tx:", sig);
  } else if (SOLANA_RPC_URL.includes("devnet")) {
    console.log(`No FUNDER_SECRET_KEY set — requesting a ${sol} SOL devnet airdrop...`);
    const sig = await conn.requestAirdrop(address, lamports);
    await conn.confirmTransaction(sig, "confirmed");
    console.log("airdrop:", sig);
  } else {
    throw new Error("Set FUNDER_SECRET_KEY in .env (airdrop is only available on devnet).");
  }

  console.log(`\nNew balance: ${(await conn.getBalance(address)) / LAMPORTS_PER_SOL} SOL`);
  console.log("\nNext: npm run sign -- --to <recipient> --sol 0.01");
}

main().catch((err) => {
  console.error("\nFund failed:", err.message);
  process.exit(1);
});
