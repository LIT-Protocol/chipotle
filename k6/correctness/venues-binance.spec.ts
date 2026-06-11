/**
 * Binance conformance as Lit Actions — the M1 gate (plan D5).
 *
 * binance.com geo-blocks US egress (HTTP 451), so everything here rides the
 * D4 egress proxy via the in-TEE Lit.Actions.proxiedFetch op (M2):
 *   LIT_VENUES_PROXY            http(s)://user:pass@host:port (non-US exit)
 *
 * Env-gated authenticated lifecycle (spot testnet — full order lifecycle):
 *   BINANCE_TESTNET_KEY / BINANCE_TESTNET_SECRET [/ BINANCE_TESTNET_KEY_TYPE]
 *
 * Usage:
 *   BASE_URL=https://…/core/v1 LIT_VENUES_PROXY=… k6 run k6/correctness/venues-binance.spec.ts
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
async function main(params) {
  const binance = LitVenues.createVenue({ venueId: 'binance', proxy: params.proxy });
  const out = {};
  try {
    const t = await binance.fetchTicker('BTC/USDT');
    out.ticker = { ok: t.last > 0, last: t.last };
  } catch (e) {
    out.ticker = { ok: false, code: e.code, httpStatus: e.httpStatus, msg: String(e.message).slice(0, 200) };
  }
  try {
    const m = await binance.fetchMarket('BTC/USDT');
    out.market = { ok: !!(m.priceIncrement && m.amountIncrement), tick: m.priceIncrement, lot: m.amountIncrement };
  } catch (e) {
    out.market = { ok: false, code: e.code };
  }
  try {
    await binance.fetchTicker('NOTACOIN/USDT');
    out.badSymbol = { ok: false, msg: 'expected bad_symbol error' };
  } catch (e) {
    out.badSymbol = { ok: e.code === 'bad_symbol', code: e.code };
  }
  return out;
}
`;

const LIFECYCLE_MAIN = `
async function main(params) {
  const binance = LitVenues.createVenue({
    venueId: 'binance',
    sandbox: true,
    proxy: params.proxy,
    credentials: { apiKey: params.key, secret: params.secret, keyType: params.keyType },
  });
  const out = {};
  const balances = await binance.fetchBalances();
  out.balances = { ok: Array.isArray(balances) };
  const m = await binance.fetchMarket('BTC/USDT');
  const t = await binance.fetchTicker('BTC/USDT');
  // Rest deep below market, sized to clear minNotional with 20% headroom.
  const halfPx = String(Math.max(1, Math.floor(t.last / 2)));
  const px = LitVenues.roundDownToIncrement(halfPx, m.priceIncrement);
  const lot = Number(m.amountIncrement);
  const minNotional = Number(m.minNotional || '5');
  const needed = (minNotional * 1.2) / Number(px);
  const decimals = (m.amountIncrement.split('.')[1] || '').length;
  const amount = (Math.ceil(needed / lot) * lot).toFixed(decimals);
  const order = await binance.createOrder({ symbol: 'BTC/USDT', side: 'buy', type: 'limit', amount, price: px });
  const open = await binance.fetchOpenOrders('BTC/USDT');
  await binance.cancelOrder(order.id, 'BTC/USDT');
  const after = await binance.fetchOpenOrders('BTC/USDT');
  return {
    ...out,
    order: { ok: !!order.id, id: order.id, status: order.status },
    open: { ok: open.some((o) => o.id === order.id), count: open.length },
    canceled: { ok: !after.some((o) => o.id === order.id) },
  };
}
`;

export default function (data: VenueSpecContext) {
  const proxy = __ENV.LIT_VENUES_PROXY;
  if (!proxy) {
    console.warn(
      "venues/binance: LIT_VENUES_PROXY not set — skipping entirely (binance.com geo-blocks the CVM's US egress; plan D4)",
    );
    return;
  }

  const pub = runVenueAction("venues/binance/public", data.usageKeyHeaders, BUNDLE, PUBLIC_MAIN, { proxy });
  if (pub) {
    checkAndLog(null, {
      "binance public ticker via in-TEE proxiedFetch": () => (pub.ticker as { ok: boolean }).ok === true,
      "binance tick/lot rules": () => (pub.market as { ok: boolean }).ok === true,
      "binance bad symbol maps to bad_symbol": () => (pub.badSymbol as { ok: boolean }).ok === true,
    }, "venues/binance/public");
  }

  const key = __ENV.BINANCE_TESTNET_KEY;
  const secret = __ENV.BINANCE_TESTNET_SECRET;
  if (!key || !secret) {
    console.warn(
      "venues/binance: BINANCE_TESTNET_KEY/SECRET not set — skipping authenticated lifecycle (M1 gate needs them)",
    );
    return;
  }
  const lifecycle = runVenueAction(
    "venues/binance/lifecycle",
    data.usageKeyHeaders,
    BUNDLE,
    LIFECYCLE_MAIN,
    { proxy, key, secret, keyType: __ENV.BINANCE_TESTNET_KEY_TYPE ?? "hmac" },
  );
  if (lifecycle) {
    checkAndLog(null, {
      "testnet balances read": () => (lifecycle.balances as { ok: boolean }).ok === true,
      "testnet limit order rested": () => (lifecycle.order as { ok: boolean }).ok === true,
      "testnet order listed in openOrders": () => (lifecycle.open as { ok: boolean }).ok === true,
      "testnet order canceled": () => (lifecycle.canceled as { ok: boolean }).ok === true,
    }, "venues/binance/lifecycle");
  }
}

export const handleSummary = warnOnHttpFailures;
