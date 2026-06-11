// Phase 1: ask the action to apply the sweep policy and request the approval.
//
// The action reads the venue balance, refuses if it can't cover the sweep,
// and otherwise requests an L2 email approval and exits. This script is "the
// requesting app" in the D6 picture, so it has one security-relevant job
// beyond plumbing: DELIVER THE OTP TO THE APPROVER OUT-OF-BAND. The email
// only carries the link; without the OTP from this terminal, the link alone
// cannot approve anything.
//
// Usage:
//   npm run request-sweep -- [amount] [asset]
//   npm run request-sweep -- 100 USDT     (the default)

const env = require("./_env");
const { runAction } = require("./_lit");
env.load();

async function main() {
  const amount = process.argv[2] || "100";
  const asset = (process.argv[3] || "USDT").toUpperCase();

  const {
    BINANCE_TESTNET_API_KEY,
    BINANCE_TESTNET_SECRET,
    APPROVER_EMAIL,
    SWEEP_DESTINATION,
    VENUE_PROXY_URL,
  } = process.env;
  for (const [name, value] of [
    ["BINANCE_TESTNET_API_KEY", BINANCE_TESTNET_API_KEY],
    ["BINANCE_TESTNET_SECRET", BINANCE_TESTNET_SECRET],
    ["APPROVER_EMAIL", APPROVER_EMAIL],
    ["SWEEP_DESTINATION", SWEEP_DESTINATION],
  ]) {
    if (!value) throw new Error(`${name} is required in .env`);
  }

  console.log(`Requesting sweep: ${amount} ${asset} -> ${SWEEP_DESTINATION}`);
  const result = await runAction("requestSweep.js", {
    venueApiKey: BINANCE_TESTNET_API_KEY,
    venueSecret: BINANCE_TESTNET_SECRET,
    proxyUrl: VENUE_PROXY_URL || null,
    approverEmail: APPROVER_EMAIL,
    asset,
    amount,
    destination: SWEEP_DESTINATION,
  });

  if (!result.requested) {
    console.error(`Action refused to request the sweep: ${result.reason}`);
    process.exit(2);
  }

  // Record the pending intent so complete-sweep.js completes the same thing
  // the human is being asked to approve.
  env.upsert("APPROVAL_ID", result.approvalId);
  env.upsert("SWEEP_AMOUNT", amount);
  env.upsert("SWEEP_ASSET", asset);

  console.log(`\nApproval requested (expires in ${result.ttlSec}s).`);
  console.log(`  summary:    ${result.summary}`);
  console.log(`  approvalId: ${result.approvalId}  (written to .env)`);
  console.log(`  free ${asset}:  ${result.free}`);

  console.log(`\n  ONE-TIME CODE (give this to the approver yourself): ${result.otp}`);
  console.log(
    "  The email they receive carries only the link — this code is the\n" +
      "  out-of-band second factor (L2). Anyone who hijacks the inbox still\n" +
      "  can't approve without it."
  );

  if (result.approvalUrl) {
    console.log(`\n  Approval link (dev server with LIT_APPROVAL_EXPOSE_LINK):`);
    console.log(`  ${result.approvalUrl}`);
  } else {
    console.log(
      `\n  The approval link was emailed to ${process.env.APPROVER_EMAIL}.\n` +
        "  (This server doesn't expose links to callers — that's the production posture.)"
    );
  }

  console.log("\nOnce approved:  npm run complete-sweep");
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
