// Local price poller: invoke the stop action every POLL_INTERVAL_SEC until
// it triggers, then stop polling.
//
// This is functionally identical to the lit-triggers schedule trigger that
// `npm run arm` creates — same composed action, same params via _venue.js —
// just invoked from your machine instead of the hosted cron. Use it when you
// don't want the lit-triggers dependency, or to watch a stop fire in real
// time. Each poll is one attested action run costing ONE venue fetch while
// untriggered.
//
//   npm run poll            # loop until triggered
//   npm run poll -- --once  # single tick (wire this to your own crontab)

const env = require("./_env");
const { runAction } = require("./_lit");
const { buildParams } = require("./_venue");

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  env.load();
  const once = process.argv.includes("--once");
  const intervalSec = Number(process.env.POLL_INTERVAL_SEC || 30);
  const params = buildParams();

  console.log(
    `Polling ${params.symbol} on ${params.venueId}${params.sandbox ? " (sandbox)" : ""}: ` +
      `sell ${params.amount} when last <= ${params.stopPrice}, floor ${params.floorPrice}` +
      (params.dryRun ? " [DRY RUN]" : "")
  );

  let first = true;
  for (;;) {
    // First call may hit usage-key grant propagation (right after setup);
    // _lit.js only ever retries those auth errors, never a trading attempt.
    const out = await runAction(params, { retries: first ? 3 : 0 });
    first = false;

    if (!out.triggered) {
      console.log(`  ${new Date().toISOString()} last=${out.last} > stop=${out.stopPrice} — holding`);
    } else if (out.refused) {
      console.log(`  ✗ TRIGGERED but REFUSED by policy: ${out.refused}`);
      console.log("  The market is below your floor — the action will keep refusing. Intervene manually.");
    } else if (out.alreadyPlaced) {
      console.log(`  ✓ TRIGGERED — order already resting venue-side (clientOrderId ${out.clientOrderId}). Nothing to do.`);
    } else if (out.dryRun) {
      console.log(`  ✓ TRIGGERED at last=${out.last} [DRY RUN] — would place: ${JSON.stringify(out.wouldPlace)}`);
    } else if (out.sold) {
      console.log(`  ✓ TRIGGERED at last=${out.last} — SOLD: ${JSON.stringify(out.order)}`);
    } else {
      console.log(`  ✓ TRIGGERED at last=${out.last} — no-op: ${out.note || JSON.stringify(out)}`);
    }

    if (out.triggered || once) return;
    await sleep(intervalSec * 1000);
  }
}

main().catch((err) => {
  console.error("\nPoll failed:", err.message);
  console.error("Not retrying automatically — a triggered attempt may already have");
  console.error("placed the sell. Check open orders / trades on the venue first.");
  process.exit(1);
});
