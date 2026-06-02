// Run the interactive distributed key generation between this machine (the
// user) and the Lit Action (party 1).
//
//   node scripts/keygen.js           2-of-3 (default): user holds party 0 (hot)
//                                     and party 2 (cold recovery); Lit holds party 1.
//   node scripts/keygen.js --basic   2-of-2: user holds party 0, Lit holds party 1.
//                                     No recovery share — lose either and it's gone.
//
// The default is 2-of-3 because it buys a self-custody escape hatch: the user
// holds 2 of 3 shares, so if Lit ever disappears, hot + cold can still sign
// (see `npm run sign -- --recovery`). Day-to-day signing is still hot + Lit; the
// cold share stays offline. The `--basic` 2-of-2 is the minimal variant.
//
// Output:
//   * Hot store ../.mpc-store.json — hot share (party 0) + the action's keyshare
//     sealed to its CID + public key/address.
//   * 2-of-3 only: cold share (party 2) written to ../.mpc-cold-share.json —
//     MOVE THIS OFFLINE. It's only needed for recovery signing.
//   * MPC_PUBLIC_KEY / VAULT_SIGNER_ADDRESS written to .env.

const env = require("./_env");
env.load();

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
    ? { participants: 3, threshold: 2, userParties: [0, 2] } // hot + cold; Lit = 1
    : { participants: 2, threshold: 2, userParties: [0] };
  const scheme = recovery ? "2-of-3 (hot + Lit + cold recovery)" : "2-of-2 (hot + Lit, no recovery)";

  console.log(`Running ${scheme} distributed key generation (5 rounds)...`);
  // A retry is a fresh, independent DKG run, so on failure we restart the whole
  // keygen with backoff. This guards against ordinary transient network errors.
  // It also used to cover a node-side js_params-caching bug that made the
  // multi-peer 2-of-3 DKG fail intermittently ("Missing message" / "Invalid
  // commitment hash"); that's fixed on prod now (see the README), so failures
  // here should be rare.
  const ATTEMPTS = 3;
  let result;
  for (let attempt = 1; attempt <= ATTEMPTS; attempt++) {
    try {
      result = await mpc.keygen({ ...cfg, onRound: (r) => process.stdout.write(`  attempt ${attempt}/${ATTEMPTS}, round ${r}/5   \r`) });
      break;
    } catch (err) {
      if (attempt === ATTEMPTS) throw err;
      const backoff = Math.min(1000 * 2 ** (attempt - 1), 15000);
      console.log(`\n  attempt ${attempt} failed (${err.message.slice(0, 50)}…); retrying in ${backoff / 1000}s`);
      await new Promise((r) => setTimeout(r, backoff));
    }
  }
  console.log("\n");

  // Hot store: party 0 share + sealed action keyshare. Never persist the cold
  // share here — it goes to its own file the user moves offline.
  const saved = store.save({
    participants: result.participants,
    threshold: result.threshold,
    actionParty: result.actionParty,
    hotParty: 0,
    hotShare: result.userShares[0],
    encActionKeyshare: result.encActionKeyshare,
    publicKey: result.publicKey,
    address: result.address,
    chainPath: result.chainPath,
  });

  env.upsert("MPC_PUBLIC_KEY", result.publicKey);
  env.upsert("VAULT_SIGNER_ADDRESS", result.address);

  console.log("✓ Key generation complete. The full key was never assembled anywhere.\n");
  console.log("  Scheme:               ", scheme);
  console.log("  Public key:           ", result.publicKey);
  console.log("  EVM address (signer): ", result.address);
  console.log("  Hot store written to: ", saved);

  if (recovery) {
    const coldSaved = store.saveCold({
      coldParty: 2,
      coldShare: result.userShares[2],
      publicKey: result.publicKey,
      address: result.address,
    });
    console.log("  Cold recovery share:  ", coldSaved);
    console.log("\n⚠️  MOVE THE COLD SHARE OFFLINE (cold storage / a different device) and");
    console.log("    remove it from this machine. It is NOT needed for normal signing —");
    console.log("    only for `npm run sign -- --recovery` if Lit ever becomes unavailable.");
  }

  console.log("\nNext:");
  console.log("  npm run deploy:baseSepolia   # deploy a vault controlled by this address");
}

main().catch((err) => {
  console.error("\nKeygen failed:", err.message);
  process.exit(1);
});
