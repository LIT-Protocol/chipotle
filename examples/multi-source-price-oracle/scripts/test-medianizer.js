// Zero-dep test harness that mirrors the medianizer logic in
// action/priceOracle.js. Skips the Lit envelope (no signing) so you can
// validate the multi-source fetch + median + spread checks before
// touching any chain.
//
// Usage:
//   node scripts/test-medianizer.js                 # defaults to ETH
//   node scripts/test-medianizer.js --asset BTC

const SYMBOLS = {
  ETH: { coinbase: "ETH-USD", kraken: "ETHUSD", krakenKey: "XETHZUSD", bitstamp: "ethusd" },
  BTC: { coinbase: "BTC-USD", kraken: "XBTUSD", krakenKey: "XXBTZUSD", bitstamp: "btcusd" },
  SOL: { coinbase: "SOL-USD", kraken: "SOLUSD", krakenKey: "SOLUSD",   bitstamp: "solusd" },
};

function parseArgs() {
  const out = { asset: "ETH" };
  for (let i = 2; i < process.argv.length; i += 2) {
    out[process.argv[i].replace(/^--/, "")] = process.argv[i + 1];
  }
  return out;
}

async function coinbase(symbols) {
  const res = await fetch(`https://api.coinbase.com/v2/prices/${symbols.coinbase}/spot`);
  const body = await res.json();
  return Number(body.data.amount);
}
async function kraken(symbols) {
  const res = await fetch(`https://api.kraken.com/0/public/Ticker?pair=${symbols.kraken}`);
  const body = await res.json();
  const entry = body.result[symbols.krakenKey] || Object.values(body.result)[0];
  return Number(entry.c[0]);
}
async function bitstamp(symbols) {
  const res = await fetch(`https://www.bitstamp.net/api/v2/ticker/${symbols.bitstamp}/`);
  const body = await res.json();
  return Number(body.last);
}

async function main() {
  const { asset } = parseArgs();
  const symbols = SYMBOLS[asset];
  if (!symbols) {
    throw new Error(`unsupported asset: ${asset}. Supported: ${Object.keys(SYMBOLS).join(", ")}`);
  }
  console.log(`Fetching ${asset}/USD from 3 sources...\n`);

  const settled = await Promise.allSettled([
    coinbase(symbols).then((p) => ({ name: "coinbase", price: p })),
    kraken(symbols).then((p) => ({ name: "kraken",   price: p })),
    bitstamp(symbols).then((p) => ({ name: "bitstamp", price: p })),
  ]);

  const successful = [];
  const failed = [];
  for (const r of settled) {
    if (r.status === "fulfilled" && Number.isFinite(r.value.price) && r.value.price > 0) {
      successful.push(r.value);
    } else {
      failed.push(r.status === "rejected" ? r.reason.message : "invalid price");
    }
  }

  successful.forEach((s) =>
    console.log(`  ${s.name.padEnd(10)} $${s.price.toFixed(2)}`)
  );
  if (failed.length) {
    console.log("\nFailed:");
    failed.forEach((f) => console.log(`  ${f}`));
  }

  if (successful.length < 2) {
    console.error("\nFAILED: fewer than 2 sources succeeded.");
    process.exit(1);
  }

  const prices = successful.map((s) => s.price).sort((a, b) => a - b);
  const median =
    prices.length % 2 === 1
      ? prices[(prices.length - 1) / 2]
      : (prices[prices.length / 2 - 1] + prices[prices.length / 2]) / 2;
  const spreadBps = Math.round(((prices[prices.length - 1] - prices[0]) / median) * 10000);

  console.log(`\nMedian:  $${median.toFixed(2)}`);
  console.log(`Spread:  ${spreadBps} bps (max-min as % of median)`);
  console.log(
    `On-chain integer (8 decimals): ${BigInt(Math.round(median * 1e8)).toString()}`
  );

  if (spreadBps > 100) {
    console.log("\nWARNING: spread exceeds 1% — the action would refuse to sign.");
    process.exit(2);
  } else {
    console.log("\nPASSED: action would sign this price.");
  }
}

main().catch((err) => {
  console.error("\nERROR:", err.message);
  process.exit(1);
});
