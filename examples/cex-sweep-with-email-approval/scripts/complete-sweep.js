// Phase 2: ask the action to check the approval and complete the sweep.
//
// The action calls Lit.Actions.checkEmailApproval; the runtime verifies the
// approval server's attestation IN-TEE before reporting approved — so this
// script (and the approval server itself) sit outside the trust boundary.
//
// Outcomes:
//   approved  -> the action performs the gated step and returns the
//                attestation as the audit record (exit 0)
//   pending   -> approve the link first, then re-run; a lit-triggers
//                approval-completed webhook automates this in production (exit 3)
//   denied /
//   expired   -> the sweep is dead; run request-sweep again (exit 2)
//
// Usage:
//   npm run complete-sweep             # uses APPROVAL_ID written by phase 1
//   npm run complete-sweep -- apr_...  # or an explicit approval id

const env = require("./_env");
const { runAction } = require("./_lit");
env.load();

async function main() {
  const approvalId = process.argv[2] || process.env.APPROVAL_ID;
  if (!approvalId) {
    throw new Error("No APPROVAL_ID in .env and none given — run `npm run request-sweep` first");
  }

  const {
    BINANCE_TESTNET_API_KEY,
    BINANCE_TESTNET_SECRET,
    SWEEP_DESTINATION,
    SWEEP_AMOUNT,
    SWEEP_ASSET,
    VENUE_PROXY_URL,
  } = process.env;
  if (!SWEEP_AMOUNT || !SWEEP_ASSET) {
    throw new Error("No pending intent in .env (SWEEP_AMOUNT / SWEEP_ASSET) — run `npm run request-sweep` first");
  }

  console.log(`Completing sweep ${approvalId}: ${SWEEP_AMOUNT} ${SWEEP_ASSET} -> ${SWEEP_DESTINATION}`);
  const result = await runAction("completeSweep.js", {
    approvalId,
    venueApiKey: BINANCE_TESTNET_API_KEY,
    venueSecret: BINANCE_TESTNET_SECRET,
    proxyUrl: VENUE_PROXY_URL || null,
    asset: SWEEP_ASSET,
    amount: SWEEP_AMOUNT,
    destination: SWEEP_DESTINATION,
  });

  if (result.swept) {
    console.log("\n✓ Sweep completed (demo: policy verified post-approval; see README).");
    console.log(`  approver:    ${result.approver}`);
    console.log(`  assurance:   ${result.assurance}`);
    console.log(`  approved at: ${result.approvedAtMs ? new Date(result.approvedAtMs).toISOString() : "?"}`);
    console.log(`  intent:      ${JSON.stringify(result.intent)}`);
    console.log(`  attestation (in-TEE verified, keep as audit record):`);
    console.log(`  ${result.attestation}`);
    return;
  }

  if (result.status === "pending") {
    console.log("\nStill pending — the approver hasn't decided yet.");
    console.log("Open the approval link, enter the one-time code, then re-run:");
    console.log("  npm run complete-sweep");
    console.log("(In production, a lit-triggers approval-completed webhook re-invokes phase 2.)");
    process.exit(3);
  }

  console.error(`\nNot swept: status=${result.status}${result.reason ? ` (${result.reason})` : ""}`);
  console.error("Denied or expired approvals are final — run `npm run request-sweep` for a fresh one.");
  process.exit(2);
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
