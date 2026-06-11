// Run ONE TWAP tick: load state.json → execute the action → write state.json
// back. This file IS the "chained trigger" of the D7 pattern — each run is
// one small, independently attested action execution, and the state blob is
// the only thing connecting them.
//
//   npm run tick        # place the next slice (or skip, if a fence trips)
//
// Run it repeatedly (or from cron) until it prints "TWAP complete".
// Delete state.json to start a fresh TWAP with the current .env params.

const fs = require("fs");
const path = require("path");
const env = require("./_env");
const { runAction } = require("./_lit");

const STATE_FILE = path.join(__dirname, "..", "state.json");

async function main() {
  env.load();
  if (!process.env.LIT_USAGE_API_KEY) {
    throw new Error("LIT_USAGE_API_KEY missing from .env — run `npm run setup` first.");
  }

  const state = fs.existsSync(STATE_FILE)
    ? JSON.parse(fs.readFileSync(STATE_FILE, "utf8"))
    : null;
  if (state && state.done) {
    console.log("Strategy already complete — delete state.json to start a new TWAP.");
    return;
  }

  const {
    VENUE_ID = "binance",
    VENUE_SANDBOX = "true",
    SYMBOL = "BTC/USDT",
    SIDE = "buy",
    TOTAL_AMOUNT = "0.0004",
    SLICES = "4",
    MAX_SLICE_NOTIONAL = "25",
    MAX_DRIFT_BPS = "100",
    ORDER_TYPE = "market",
    DRY_RUN = "false",
  } = process.env;

  const jsParams = {
    state,
    venueId: VENUE_ID,
    sandbox: VENUE_SANDBOX !== "false",
    // CEX credentials (binance/binanceus/coinbase)
    apiKey: process.env.VENUE_API_KEY || undefined,
    secret: process.env.VENUE_SECRET || undefined,
    keyType: process.env.VENUE_KEY_TYPE || undefined,
    // hyperliquid (pkp-eip712) — see README "Hyperliquid option"
    privateKey: process.env.VENUE_PRIVATE_KEY || undefined,
    accountAddress: process.env.VENUE_ACCOUNT_ADDRESS || undefined,
    useActionKey: process.env.VENUE_USE_ACTION_KEY === "true" || undefined,
    proxyUrl: process.env.VENUE_PROXY_URL || undefined,
    // strategy + policy (decimal strings end to end)
    symbol: SYMBOL,
    side: SIDE,
    totalAmount: TOTAL_AMOUNT,
    slices: Number(SLICES),
    maxSliceNotional: MAX_SLICE_NOTIONAL,
    maxDriftBps: Number(MAX_DRIFT_BPS),
    orderType: ORDER_TYPE,
    maxTicks: process.env.MAX_TICKS ? Number(process.env.MAX_TICKS) : undefined,
    twapId: process.env.TWAP_ID || undefined,
    dryRun: DRY_RUN === "true",
  };

  const tickNo = state ? state.ticks + 1 : 1;
  console.log(
    `Tick ${tickNo}: ${SIDE} ${SYMBOL} on ${VENUE_ID}${jsParams.sandbox ? " (sandbox)" : ""} — ` +
      `${state ? state.filledSlices : 0}/${SLICES} slices placed, remaining ${state ? state.remaining : TOTAL_AMOUNT}` +
      (jsParams.dryRun ? " [DRY RUN]" : "")
  );

  // No blind retries: a failed tick may already have placed an order
  // venue-side, so _lit.js only ever retries auth-propagation errors.
  const out = await runAction(jsParams, { retries: state ? 0 : 3 });

  if (out.state) {
    fs.writeFileSync(STATE_FILE, JSON.stringify(out.state, null, 2) + "\n");
  }

  if (out.dryRun) {
    console.log(`  DRY RUN — would place: ${JSON.stringify(out.wouldPlace)} (notional ${out.notional} @ last ${out.last})`);
  } else if (out.placed) {
    console.log(
      `  ✓ slice ${out.placed.slice}/${out.placed.of} placed: ${out.placed.status} id=${out.placed.id} ` +
        `amount=${out.placed.amount} notional=${out.placed.notional} @ last ${out.placed.last}`
    );
  } else if (out.skipped) {
    console.log(`  ▲ slice skipped (policy fence): ${out.reason}`);
  } else if (out.aborted) {
    console.log(`  ✗ strategy aborted: ${out.reason}`);
  } else if (out.note) {
    console.log(`  ${out.note}`);
  }

  if (out.state && out.state.done) {
    console.log(`\nTWAP complete: ${out.state.filledSlices} child orders, remaining ${out.state.remaining}.`);
    console.log("Order log is in state.json (.orders). Delete state.json to run another TWAP.");
  } else {
    console.log("\nNext: run `npm run tick` again for the next slice.");
    console.log('Production: wire this to a lit-triggers cron for production, see README ("Going autonomous").');
  }
}

main().catch((err) => {
  console.error("\nTick failed:", err.message);
  console.error("State was NOT advanced. Inspect before re-running — see README on retries.");
  process.exit(1);
});
