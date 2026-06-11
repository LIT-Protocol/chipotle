// Run one monitoring pass:
//   1. concatenate the lit-venues IIFE bundle + the action source (the exact
//      code whose CID setup pinned),
//   2. POST it to /lit_action with the scoped usage key — the action fetches
//      Hyperliquid funding + Coinbase spot per coin and, beyond the
//      threshold, emails ALERT_EMAIL via the server-mediated sendEmail op,
//   3. print the rows and whether an alert went out.
//
// One pass = one invocation. For a standing monitor put this action on a
// lit-triggers `schedule` trigger (see the README).
//
// Usage:
//   npm run monitor              # table output
//   npm run monitor -- --json    # raw JSON result

const env = require("./_env");
const { buildCode, runCode } = require("./_lit");
env.load();

async function main() {
  if (!process.env.LIT_USAGE_API_KEY) {
    throw new Error("LIT_USAGE_API_KEY is missing from .env — run `npm run setup` first");
  }

  const coins = (process.env.COINS || "BTC,ETH")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  const thresholdPct = Number(process.env.THRESHOLD_PCT || "20");
  const alertTo = process.env.ALERT_EMAIL || undefined;
  if (!alertTo) {
    console.log("ALERT_EMAIL not set — reporting only, no email will ever be sent.\n");
  }

  const result = await runCode(buildCode(), {
    coins,
    thresholdPct,
    alertTo,
    sandbox: process.env.HL_SANDBOX === "true",
  });

  if (process.argv.includes("--json")) {
    console.log(JSON.stringify(result, null, 2));
    return;
  }

  if (!result || !Array.isArray(result.rows)) {
    console.error("Unexpected action response:", JSON.stringify(result));
    process.exit(2);
  }

  const pad = (s, n) => String(s == null ? "-" : s).padEnd(n);
  console.log(`Funding monitor @ ${new Date(result.ts).toISOString()}  (threshold ${result.thresholdPct}%)\n`);
  console.log(
    pad("COIN", 8) + pad("FUNDING/HR", 13) + pad("ANNUALIZED%", 13) + pad("HL MARK", 12) + pad("CB SPOT", 12) + "BASIS%"
  );
  for (const r of result.rows) {
    console.log(
      pad(r.coin + (r.alert ? "*" : ""), 8) +
        pad(r.fundingHourly, 13) +
        pad(r.fundingAnnualizedPct, 13) +
        pad(r.hlMark, 12) +
        pad(r.spotUsd, 12) +
        (r.basisPct == null ? "-" : r.basisPct)
    );
    if (r.hlError) console.log(`  ${r.coin} hyperliquid error [${r.hlError.code}]: ${r.hlError.message}`);
    if (r.spotError) console.log(`  ${r.coin} coinbase error [${r.spotError.code}]: ${r.spotError.message}`);
  }

  console.log("");
  if (result.alerted) {
    console.log(`* beyond threshold — alert email sent to ${alertTo}`);
  } else if (result.rows.some((r) => r.alert)) {
    console.log("* beyond threshold — no email sent (ALERT_EMAIL not set)");
  } else {
    console.log("All coins within threshold — no alert.");
  }
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
