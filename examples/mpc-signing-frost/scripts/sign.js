// Sign + submit a Solana transfer with the threshold key. Two signing modes,
// two targets:
//
//   Modes:
//     (default)    hot share + Lit Action  — the normal path (2 FROST rounds).
//     --recovery   hot share + cold share  — NO Lit Action; entirely local.
//                  The 2-of-3 self-custody escape hatch (the default `keygen`
//                  writes the cold share; restore it before recovery signing).
//   Targets:
//     (default)    build + submit a SystemProgram.transfer on-chain.
//     --dry        produce + verify the signature locally; no chain, no funds.
//
// Either way the result is a standard 64-byte Ed25519 signature Solana verifies
// natively — the chain has no idea the key is shared.
//
// Usage:
//   node scripts/sign.js --to <recipient> --sol 0.01
//   node scripts/sign.js --to <recipient> --sol 0.01 --recovery
//   node scripts/sign.js --dry            (normal-path Lit signing, no chain)
//   node scripts/sign.js --dry --recovery (hot+cold signing, no chain)

const env = require("./_env");
env.load();

const { Connection, PublicKey, SystemProgram, Transaction, LAMPORTS_PER_SOL } = require("@solana/web3.js");
const nacl = require("tweetnacl");
const bs58 = require("bs58");
const { MpcClient } = require("../client/mpcClient");
const store = require("../client/store");

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_USAGE_API_KEY,
  MPC_PKP_ADDRESS,
  SOLANA_RPC_URL = "https://api.devnet.solana.com",
} = process.env;

function parseArgs() {
  const out = {};
  const a = process.argv.slice(2);
  for (let i = 0; i < a.length; i++) {
    const key = a[i].replace(/^--/, "");
    const next = a[i + 1];
    if (next === undefined || next.startsWith("--")) out[key] = true;
    else { out[key] = next; i++; }
  }
  return out;
}

// Produce a 64-byte Ed25519 signature over `message` via hot+Lit or hot+cold.
async function produceSig(recovery, message) {
  const hot = store.load();
  if (recovery) {
    const cold = store.loadCold();
    console.log("Recovery signing: hot + cold shares, NO Lit Action involved...");
    return MpcClient.signLocal({
      shares: [
        { bytes: hot.hotShare, id: hot.hotId ?? 1 },
        { bytes: cold.coldShare, id: cold.coldId ?? 3 },
      ],
      verifyingKey: hot.verifyingKey,
      threshold: hot.threshold ?? 2,
      message,
    });
  }
  console.log("Signing: hot share + Lit Action (2 FROST rounds)...");
  const mpc = new MpcClient({ apiBase: LIT_API_BASE, usageApiKey: LIT_USAGE_API_KEY, pkpId: MPC_PKP_ADDRESS });
  const sig = await mpc.sign({
    hotShare: hot.hotShare,
    encActionKeyshare: hot.encActionKeyshare,
    verifyingKey: hot.verifyingKey,
    threshold: hot.threshold ?? 2,
    message,
    onRound: (r) => process.stdout.write(`  round ${r}/2\r`),
  });
  process.stdout.write("\n");
  return sig;
}

async function main() {
  const args = parseArgs();
  const recovery = "recovery" in args;
  const dry = "dry" in args;

  if (!recovery) {
    for (const k of ["LIT_USAGE_API_KEY", "MPC_PKP_ADDRESS"]) {
      if (!process.env[k]) throw new Error(`${k} is required for hot+Lit signing`);
    }
  }

  const hot = store.load();
  const from = new PublicKey(Buffer.from(hot.solanaPubkey, "base64"));
  const pubkeyBytes = from.toBytes();

  let conn, blockhash, lastValidBlockHeight, submit = null;
  const to = args.to ? new PublicKey(args.to) : from; // dry default: self
  const lamports = Math.round(parseFloat(args.sol || "0") * LAMPORTS_PER_SOL);

  if (dry) {
    blockhash = PublicKey.default.toBase58(); // dummy; not submitted
    console.log(`Dry run (${recovery ? "recovery: hot+cold" : "normal: hot+Lit"}) — no chain.\n`);
  } else {
    if (!args.to) throw new Error("Usage: node scripts/sign.js --to <recipient> --sol 0.01 [--recovery] [--dry]");
    conn = new Connection(SOLANA_RPC_URL, "confirmed");
    ({ blockhash, lastValidBlockHeight } = await conn.getLatestBlockhash());
    const bal = await conn.getBalance(from);
    console.log(`From:    ${from.toBase58()}  (balance ${bal / LAMPORTS_PER_SOL} SOL)`);
    console.log(`Transfer: ${lamports / LAMPORTS_PER_SOL} SOL -> ${to.toBase58()}`);
    if (bal < lamports) console.log("\n⚠️  Balance below the requested amount — fund it first (npm run fund).");
    console.log();
    submit = async (tx) => {
      const txid = await conn.sendRawTransaction(tx.serialize());
      console.log("\ntx:", txid);
      await conn.confirmTransaction({ signature: txid, blockhash, lastValidBlockHeight }, "confirmed");
      console.log("confirmed.");
    };
  }

  const tx = new Transaction({ feePayer: from, recentBlockhash: blockhash });
  tx.add(SystemProgram.transfer({ fromPubkey: from, toPubkey: to, lamports }));
  const message = tx.serializeMessage();

  const sig = await produceSig(recovery, message);

  if (!nacl.sign.detached.verify(new Uint8Array(message), new Uint8Array(sig), pubkeyBytes)) {
    throw new Error("MPC signature failed standard Ed25519 verification");
  }
  console.log(`✓ signature verifies as a standard Ed25519 sig for ${from.toBase58()}.`);
  console.log(`  signature: ${bs58.encode(sig)}`);

  if (submit) {
    tx.addSignature(from, sig);
    await submit(tx);
  }
}

main().catch((err) => {
  console.error("\nSign failed:", err.message);
  process.exit(1);
});
