// Attack 2 — over-cap fill. The compromised bot keeps the legit recipient (so
// the recipient-binding check passes) but inflates the amount past the vault's
// maxFillAmount, trying to drain more than policy allows in one shot.
//
// The policy action reads maxFillAmount from the vault and refuses to sign
// anything above it. (It would also refuse an amount above the order's own
// amount — try bumping just past 100 mUSDC to see that check fire instead.)
//
// Usage: node scripts/attack-bad-fill.js   (or: npm run attack:bad-fill)

const { ethers } = require("ethers");
const env = require("./_env");
env.load();
const { requestFillAuthorization, fillParams } = require("./_lit");

async function main() {
  // 5,000 mUSDC — well over the default 1,000 cap.
  const overCap = ethers.utils.parseUnits("5000", 6).toString();
  console.log("Compromised bot requests a 5,000 mUSDC fill (cap is far lower)...");

  const auth = await requestFillAuthorization(fillParams({ amount: overCap }));

  if (auth && auth.authorized) {
    console.error("\n✗ UNEXPECTED: policy authorized the over-cap fill. This is a bug.");
    console.error(auth);
    process.exit(1);
  }

  console.log("\n✓ Policy REJECTED the over-cap fill. No signature was produced.");
  console.log("  reason:", auth && auth.reason);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
