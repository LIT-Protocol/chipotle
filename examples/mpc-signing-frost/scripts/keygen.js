// Run the interactive FROST distributed key generation between this machine
// (the user) and the Lit Action (party id 2).
//
//   node scripts/keygen.js           2-of-3 (default): user holds party 1 (hot)
//                                     and party 3 (cold recovery); Lit holds id 2.
//   node scripts/keygen.js --basic   2-of-2: user holds party 1, Lit holds id 2.
//                                     No recovery share — lose either and it's gone.
//
// The default is 2-of-3 because it buys a self-custody escape hatch: the user
// holds 2 of 3 shares, so if Lit ever disappears, hot + cold can still sign (see
// `npm run sign -- --recovery`). Day-to-day signing is hot + Lit; the cold share
// stays offline.
//
// Output:
//   * Hot store ../.mpc-store.json — hot share (id 1) + the action's signing
//     share sealed to its CID + the group key / Solana address.
//   * 2-of-3 only: cold share (id 3) written to ../.mpc-cold-share.json —
//     MOVE THIS OFFLINE. It's only needed for recovery signing.
//   * MPC_PUBLIC_KEY / SOLANA_ADDRESS written to .env.

const env = require("./_env");
env.load();

const { PublicKey } = require("@solana/web3.js");
const { MpcClient } = require("../client/mpcClient");
const store = require("../client/store");

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_USAGE_API_KEY,
  MPC_PKP_ADDRESS,
} = process.env;

async function main() {
  const recovery = !process.argv.includes("--basic"); // 2-of-3 w/ cold recovery is the default
  for (const k of ["LIT_USAGE_API_KEY", "MPC_PKP_ADDRESS"]) {
    if (!process.env[k]) throw new Error(`${k} is required (run \`npm run setup\` first)`);
  }

  if (store.exists()) console.log("Note: an existing keyshare will be overwritten.\n");

  const mpc = new MpcClient({ apiBase: LIT_API_BASE, usageApiKey: LIT_USAGE_API_KEY, pkpId: MPC_PKP_ADDRESS });

  const cfg = recovery
    ? { allIds: [1, 2, 3], threshold: 2, userParties: [1, 3] } // hot + cold; Lit = 2
    : { allIds: [1, 2], threshold: 2, userParties: [1] };
  const scheme = recovery ? "2-of-3 (hot + Lit + cold recovery)" : "2-of-2 (hot + Lit, no recovery)";

  console.log(`Running ${scheme} FROST distributed key generation (3 rounds)...`);
  // A retry is a fresh, independent DKG run, so on failure we restart the whole
  // keygen with backoff — guards against ordinary transient network errors.
  const ATTEMPTS = 3;
  let result;
  for (let attempt = 1; attempt <= ATTEMPTS; attempt++) {
    try {
      result = await mpc.keygen({ ...cfg, onRound: (r) => process.stdout.write(`  attempt ${attempt}/${ATTEMPTS}, round ${r}/3   \r`) });
      break;
    } catch (err) {
      if (attempt === ATTEMPTS) throw err;
      const backoff = Math.min(1000 * 2 ** (attempt - 1), 15000);
      console.log(`\n  attempt ${attempt} failed (${err.message.slice(0, 60)}…); retrying in ${backoff / 1000}s`);
      await new Promise((r) => setTimeout(r, backoff));
    }
  }
  console.log("\n");

  const pubkeyBytes = Buffer.from(result.solanaPubkey, "base64");
  const address = new PublicKey(pubkeyBytes).toBase58();

  const saved = store.save({
    allIds: result.allIds,
    threshold: result.threshold,
    actionId: result.actionId,
    hotId: 1,
    hotShare: result.userShares[1],
    encActionKeyshare: result.encActionKeyshare,
    verifyingKey: result.verifyingKey,
    solanaPubkey: result.solanaPubkey,
    address,
  });

  env.upsert("MPC_PUBLIC_KEY", "0x" + pubkeyBytes.toString("hex"));
  env.upsert("SOLANA_ADDRESS", address);

  console.log("✓ Key generation complete. The full key was never assembled anywhere.\n");
  console.log("  Scheme:               ", scheme);
  console.log("  Solana address:       ", address);
  console.log("  Hot store written to: ", saved);

  if (recovery) {
    const coldSaved = store.saveCold({
      coldId: 3,
      coldShare: result.userShares[3],
      verifyingKey: result.verifyingKey,
      solanaPubkey: result.solanaPubkey,
      address,
    });
    console.log("  Cold recovery share:  ", coldSaved);
    console.log("\n⚠️  MOVE THE COLD SHARE OFFLINE (cold storage / a different device) and");
    console.log("    remove it from this machine. It is NOT needed for normal signing —");
    console.log("    only for `npm run sign -- --recovery` if Lit ever becomes unavailable.");
  }

  console.log("\nNext:");
  console.log("  npm run fund                              # send the address a little devnet SOL");
  console.log("  npm run sign -- --to <addr> --sol 0.01    # hot + Lit sign + submit a transfer");
}

main().catch((err) => {
  console.error("\nKeygen failed:", err.message);
  process.exit(1);
});
