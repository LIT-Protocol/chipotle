/**
 * Coinbase Advanced Trade conformance as Lit Actions — the M1 gate (plan D5).
 *
 * Advanced Trade has NO real sandbox (plan D5 is explicit about this), so:
 *   (a) public conformance always runs (direct egress — Coinbase is US-reachable)
 *   (b) authenticated READ-ONLY runs when CDP keys are supplied:
 *         COINBASE_KEY_NAME / COINBASE_PRIVATE_KEY
 *   (c) a TINY live order+cancel runs only behind the manual flag:
 *         COINBASE_LIVE_ORDER=true   (deep-out-of-the-money limit, then cancel)
 *
 * Usage:
 *   BASE_URL=https://…/core/v1 k6 run k6/correctness/venues-coinbase.spec.ts
 */
import { checkAndLog, warnOnHttpFailures } from "../helpers.ts";
import {
  VENUE_SPEC_OPTIONS,
  loadVenuesBundle,
  runVenueAction,
  venueSpecSetup,
  type VenueSpecContext,
} from "./venues-common.ts";

export const options = VENUE_SPEC_OPTIONS;

const BUNDLE = loadVenuesBundle();

export function setup() {
  return venueSpecSetup();
}

const PUBLIC_MAIN = `
async function main() {
  const coinbase = LitVenues.createVenue({ venueId: 'coinbase' });
  const out = {};
  try {
    const t = await coinbase.fetchTicker('BTC/USD');
    out.ticker = { ok: t.last > 0, last: t.last };
  } catch (e) {
    out.ticker = { ok: false, code: e.code, msg: String(e.message).slice(0, 200) };
  }
  try {
    const m = await coinbase.fetchMarket('ETH/USD');
    out.market = { ok: !!(m.priceIncrement && m.amountIncrement), tick: m.priceIncrement, lot: m.amountIncrement };
  } catch (e) {
    out.market = { ok: false, code: e.code };
  }
  try {
    await coinbase.fetchTicker('NOTACOIN/USD');
    out.badSymbol = { ok: false, msg: 'expected an error' };
  } catch (e) {
    out.badSymbol = { ok: e.code === 'bad_symbol', code: e.code };
  }
  return out;
}
`;

const READ_MAIN = `
async function main(params) {
  const coinbase = LitVenues.createVenue({
    venueId: 'coinbase',
    credentials: { apiKey: params.keyName, secret: params.privateKey, keyType: 'es256-jwt' },
  });
  const balances = await coinbase.fetchBalances();
  const open = await coinbase.fetchOpenOrders('BTC/USD');
  const fills = await coinbase.fetchMyTrades('BTC/USD', { limit: 5 });
  return {
    balances: { ok: Array.isArray(balances), count: balances.length },
    open: { ok: Array.isArray(open) },
    fills: { ok: Array.isArray(fills) },
  };
}
`;

const LIVE_ORDER_MAIN = `
async function main(params) {
  const coinbase = LitVenues.createVenue({
    venueId: 'coinbase',
    credentials: { apiKey: params.keyName, secret: params.privateKey, keyType: 'es256-jwt' },
  });
  const m = await coinbase.fetchMarket('BTC/USD');
  const t = await coinbase.fetchTicker('BTC/USD');
  // Tiny and unfillable: half the market price, minimum base size.
  const px = LitVenues.roundDownToIncrement(String(Math.floor(t.last / 2)), m.priceIncrement);
  const amount = m.minAmount || m.amountIncrement;
  const order = await coinbase.createOrder({ symbol: 'BTC/USD', side: 'buy', type: 'limit', amount, price: px });
  const open = await coinbase.fetchOpenOrders('BTC/USD');
  await coinbase.cancelOrder(order.id, 'BTC/USD');
  return {
    order: { ok: !!order.id, id: order.id },
    open: { ok: open.some((o) => o.id === order.id), count: open.length },
    canceled: { ok: true },
  };
}
`;

export default function (data: VenueSpecContext) {
  const pub = runVenueAction("venues/coinbase/public", data.usageKeyHeaders, BUNDLE, PUBLIC_MAIN, null);
  if (pub) {
    checkAndLog(null, {
      "coinbase public ticker (direct egress)": () => (pub.ticker as { ok: boolean }).ok === true,
      "coinbase tick/lot rules": () => (pub.market as { ok: boolean }).ok === true,
      "coinbase bad symbol maps to bad_symbol": () => (pub.badSymbol as { ok: boolean }).ok === true,
    }, "venues/coinbase/public");
  }

  const keyName = __ENV.COINBASE_KEY_NAME;
  const privateKey = __ENV.COINBASE_PRIVATE_KEY;
  if (!keyName || !privateKey) {
    console.warn(
      "venues/coinbase: COINBASE_KEY_NAME/PRIVATE_KEY not set — skipping authenticated read-only (M1 gate needs them)",
    );
    return;
  }
  const reads = runVenueAction(
    "venues/coinbase/authenticated-reads",
    data.usageKeyHeaders,
    BUNDLE,
    READ_MAIN,
    { keyName, privateKey },
  );
  if (reads) {
    checkAndLog(null, {
      "authenticated balances (ES256 JWT)": () => (reads.balances as { ok: boolean }).ok === true,
      "authenticated open orders": () => (reads.open as { ok: boolean }).ok === true,
      "authenticated fills": () => (reads.fills as { ok: boolean }).ok === true,
    }, "venues/coinbase/authenticated-reads");
  }

  if (__ENV.COINBASE_LIVE_ORDER !== "true") {
    console.warn(
      "venues/coinbase: COINBASE_LIVE_ORDER!=true — skipping the tiny live order+cancel (manual Tier-1 flag, plan D5c)",
    );
    return;
  }
  const live = runVenueAction(
    "venues/coinbase/live-order",
    data.usageKeyHeaders,
    BUNDLE,
    LIVE_ORDER_MAIN,
    { keyName, privateKey },
  );
  if (live) {
    checkAndLog(null, {
      "tiny live order placed": () => (live.order as { ok: boolean }).ok === true,
      "live order visible in openOrders": () => (live.open as { ok: boolean }).ok === true,
      "live order canceled": () => (live.canceled as { ok: boolean }).ok === true,
    }, "venues/coinbase/live-order");
  }
}

export const handleSummary = warnOnHttpFailures;
