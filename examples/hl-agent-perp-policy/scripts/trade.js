// Invoke the policy action: it enforces the fences, then signs and places the
// order as the venue-approved agent — the key never leaves the TEE.
//
// Usage:
//   npm run trade -- <side> <amount> <coin> [price] [Nx] [reduce-only]
//
//   npm run trade -- buy 0.01 ETH                # market order, 1x
//   npm run trade -- buy 0.01 ETH 2000           # GTC limit @ 2000
//   npm run trade -- buy 0.01 ETH 2000 3x        # ... with 3x leverage
//   npm run trade -- sell 0.01 ETH reduce-only   # market, may only shrink the position
//   npm run trade -- buy 1 BTC                   # refused: notional over the policy cap
//
// Tail arguments, any order: a plain number is the limit price, "Nx" is
// leverage (the action clamps it to the policy max), "reduce-only" forces a
// position-shrinking order.

const crypto = require("crypto");
const env = require("./_env");
const { runAction } = require("./_lit");
env.load();

function parseArgs(argv) {
  const [side, amount, coin, ...rest] = argv;
  if (!side || !amount || !coin) {
    throw new Error("Usage: npm run trade -- <side> <amount> <coin> [price] [Nx] [reduce-only]");
  }
  let price;
  let leverage;
  let reduceOnly = false;
  for (const arg of rest) {
    if (/^reduce-only$/i.test(arg)) reduceOnly = true;
    else if (/^\d+x$/i.test(arg)) leverage = parseInt(arg, 10);
    else if (/^\d+(\.\d+)?$/.test(arg)) price = arg;
    else throw new Error(`unrecognized argument "${arg}"`);
  }
  return { side, amount, coin: coin.toUpperCase(), price, leverage, reduceOnly };
}

async function main() {
  const { side, amount, coin, price, leverage, reduceOnly } = parseArgs(process.argv.slice(2));

  const masterAddress = process.env.HL_MASTER_ADDRESS;
  if (!masterAddress) {
    throw new Error("HL_MASTER_ADDRESS missing in .env — run `npm run approve-agent` first");
  }

  // A fresh cloid per attempt: idempotency + audit handle. Hyperliquid
  // requires exactly 0x + 32 lowercase hex chars (128-bit).
  const clientOrderId = "0x" + crypto.randomBytes(16).toString("hex");

  console.log(
    `Asking the action to ${side} ${amount} ${coin} ` +
      `${price ? `@ ${price} (GTC limit)` : "(market)"}${leverage ? ` ${leverage}x` : ""}` +
      `${reduceOnly ? " reduce-only" : ""}  [cloid ${clientOrderId}]`
  );

  const result = await runAction({
    action: "trade",
    masterAddress,
    coin,
    side,
    amount,
    price: price || null,
    leverage: leverage || null,
    reduceOnly,
    clientOrderId,
  });

  if (!result.authorized) {
    console.error(`Action refused to trade: ${result.reason}`);
    process.exit(2);
  }

  const { order, policy, positions } = result;
  console.log("\n✓ Order placed by the agent (signed in-TEE).");
  console.log(`  policy applied: leverage ${policy.leverage}x` +
    (policy.leverageRequested !== policy.leverage ? ` (requested ${policy.leverageRequested}x, clamped)` : "") +
    `, reduceOnly ${policy.reduceOnly}, notional ~$${policy.orderNotionalUsd} (cap $${policy.maxNotionalUsd})`);
  console.log(`  order: id=${order.id} status=${order.status} ${order.side} ${order.amount} ${order.symbol} @ ${order.price}` +
    (order.filled && order.filled !== "0" ? ` (filled ${order.filled})` : ""));
  if (positions.length === 0) {
    console.log("  open positions: none");
  } else {
    for (const p of positions) {
      console.log(
        `  position: ${p.side} ${p.size} ${p.symbol}` +
          (p.entryPrice ? ` entry ${p.entryPrice}` : "") +
          (p.leverage ? ` ${p.leverage}x` : "") +
          (p.unrealizedPnl ? ` uPnL ${p.unrealizedPnl}` : "")
      );
    }
  }
  console.log(`\nInspect the account at https://app.hyperliquid-testnet.xyz (master ${masterAddress}).`);
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
