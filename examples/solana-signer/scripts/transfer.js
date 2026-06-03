// Send SOL from the action's keyless wallet:
//   1. Build a SystemProgram transfer transaction (fee payer = action wallet)
//      and serialize its message — this is the canonical byte string both
//      sides agree on.
//   2. Ask the action to inspect that message and ed25519-sign it. The action
//      only signs a single capped transfer whose fee payer is its own address.
//   3. Attach the returned signature and broadcast to devnet.
//
// The action never sees a private key leave the TEE and never trusts us to
// tell it what it's signing — it parses the message itself. We supply the
// recipient redundantly only so a mismatch fails loudly.
//
// Usage:
//   npm run transfer -- <recipientBase58> <amountSol>
//   npm run transfer -- 9xQ...recipient 0.01

const {
  Connection,
  PublicKey,
  Transaction,
  SystemProgram,
  LAMPORTS_PER_SOL,
  clusterApiUrl,
} = require("@solana/web3.js");
const env = require("./_env");
const { runAction } = require("./_lit");
env.load();

async function main() {
  const recipient = process.argv[2];
  const amountSol = Number(process.argv[3]);
  if (!recipient || !Number.isFinite(amountSol) || amountSol <= 0) {
    throw new Error("Usage: npm run transfer -- <recipientBase58> <amountSol>");
  }

  const rpcUrl = process.env.SOLANA_RPC_URL || clusterApiUrl("devnet");
  const connection = new Connection(rpcUrl, "confirmed");

  const from = new PublicKey(
    process.env.SOLANA_ADDRESS || (await runAction({ action: "address" })).address
  );
  const to = new PublicKey(recipient);
  const lamports = Math.round(amountSol * LAMPORTS_PER_SOL);

  // ---- 1. Build the canonical transfer message ----------------------------
  const { blockhash } = await connection.getLatestBlockhash("confirmed");
  const tx = new Transaction({ feePayer: from, recentBlockhash: blockhash });
  tx.add(SystemProgram.transfer({ fromPubkey: from, toPubkey: to, lamports }));
  const messageBytes = tx.serializeMessage();

  // ---- 2. Have the action inspect + sign it -------------------------------
  console.log(`Asking the action to sign: ${amountSol} SOL  ${from.toBase58()} -> ${to.toBase58()}`);
  const result = await runAction({
    action: "sign",
    message: messageBytes.toString("base64"),
    recipient: to.toBase58(),
  });
  if (!result || !result.authorized) {
    console.error("Action declined to sign:", result && result.reason);
    process.exit(2);
  }

  // ---- 3. Attach the signature and broadcast ------------------------------
  // addSignature re-derives the message and verifies the 64-byte signature
  // against the fee payer's key, so a bad signature fails here, locally,
  // rather than on-chain.
  tx.addSignature(from, Buffer.from(result.signature, "base64"));
  const raw = tx.serialize();

  const txid = await connection.sendRawTransaction(raw);
  console.log(`broadcast tx: ${txid}`);
  await connection.confirmTransaction(txid, "confirmed");
  console.log(`confirmed: https://explorer.solana.com/tx/${txid}?cluster=devnet`);
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
