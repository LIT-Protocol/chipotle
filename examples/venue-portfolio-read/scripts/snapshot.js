// Take an attested multi-venue balance snapshot:
//   1. concatenate the lit-venues IIFE bundle + the action source (the exact
//      code whose CID setup pinned),
//   2. POST it to /lit_action with the scoped usage key — the action decrypts
//      the sealed credentials in-TEE and fetches balances per venue,
//   3. pretty-print the per-venue balances and the per-asset totals.
//
// This script never sees a venue API key — only the ciphertext from .env.
//
// Usage:
//   npm run snapshot             # table output
//   npm run snapshot -- --json   # raw JSON snapshot

const env = require("./_env");
const { buildCode, runCode } = require("./_lit");
env.load();

async function main() {
  for (const k of ["LIT_USAGE_API_KEY", "VAULT_PKP_ADDRESS", "SEALED_VENUE_CREDENTIALS"]) {
    if (!process.env[k]) {
      throw new Error(`${k} is missing from .env — run \`npm run setup\` first`);
    }
  }

  const snap = await runCode(buildCode(), {
    pkpId: process.env.VAULT_PKP_ADDRESS,
    sealedCreds: process.env.SEALED_VENUE_CREDENTIALS,
  });

  if (process.argv.includes("--json")) {
    console.log(JSON.stringify(snap, null, 2));
    process.exit(snap && snap.ok ? 0 : 2);
  }

  if (!snap || snap.venues === undefined) {
    console.error("Unexpected action response:", JSON.stringify(snap));
    process.exit(2);
  }

  console.log(`Portfolio snapshot @ ${new Date(snap.ts).toISOString()}\n`);
  const pad = (s, n) => String(s).padEnd(n);
  console.log(pad("VENUE", 14) + pad("ASSET", 8) + pad("FREE", 20) + "TOTAL");

  for (const [venueId, v] of Object.entries(snap.venues)) {
    if (!v.ok) {
      console.log(pad(venueId, 14) + `ERROR [${v.error.code}] ${v.error.message}`);
      continue;
    }
    if (v.balances.length === 0) {
      console.log(pad(venueId, 14) + "(no non-zero balances)");
      continue;
    }
    for (const b of v.balances) {
      console.log(pad(venueId, 14) + pad(b.asset, 8) + pad(b.free, 20) + b.total);
    }
  }

  console.log("");
  for (const [asset, total] of Object.entries(snap.totals)) {
    console.log(pad("TOTAL", 14) + pad(asset, 8) + total);
  }

  if (!snap.ok) {
    console.error("\nNo venue returned balances — see errors above.");
    process.exit(2);
  }
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
