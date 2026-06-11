/**
 * Hyperliquid conformance as Lit Actions — the M2.5 gate (plan D8).
 *
 * Always: public conformance (ticker, market rules, funding, error taxonomy)
 * over DIRECT CVM egress — doubling as the egress-posture measurement (no
 * exchange-side IP allowlist exists; this answers whether the API blocks us).
 *
 * Env-gated authenticated lifecycle (testnet — full order lifecycle in CI is
 * possible here, unlike Coinbase):
 *   HYPERLIQUID_TESTNET_KEY      secp256k1 hex key of a faucet-funded testnet account
 *   HYPERLIQUID_ACCOUNT_ADDRESS  optional master address (agent mode)
 *
 * Usage:
 *   BASE_URL=https://…/core/v1 k6 run k6/correctness/venues-hyperliquid.spec.ts
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
  const hl = LitVenues.createVenue({ venueId: 'hyperliquid' });
  const out = {};
  try {
    const t = await hl.fetchTicker('BTC');
    out.ticker = { ok: t.last > 0, last: t.last };
  } catch (e) {
    out.ticker = { ok: false, code: e.code, httpStatus: e.httpStatus, msg: String(e.message).slice(0, 200) };
  }
  try {
    const m = await hl.fetchMarket('ETH');
    out.market = { ok: !!(m.priceIncrement && m.amountIncrement), szRules: m.info };
  } catch (e) {
    out.market = { ok: false, code: e.code };
  }
  try {
    const f = await hl.fetchFundingRate('BTC');
    out.funding = { ok: typeof f.fundingRate === 'string' };
  } catch (e) {
    out.funding = { ok: false, code: e.code };
  }
  try {
    await hl.fetchTicker('NOTACOIN');
    out.badSymbol = { ok: false, msg: 'expected bad_symbol error' };
  } catch (e) {
    out.badSymbol = { ok: e.code === 'bad_symbol', code: e.code };
  }
  return out;
}
`;

const LIFECYCLE_MAIN = `
async function main(params) {
  const hl = LitVenues.createVenue({
    venueId: 'hyperliquid',
    sandbox: true,
    credentials: {
      keyType: 'pkp-eip712',
      privateKey: params.key,
      accountAddress: params.accountAddress || undefined,
    },
  });
  const out = {};
  const balances = await hl.fetchBalances();
  out.balances = { ok: balances.length === 1, free: balances[0] && balances[0].free };
  const m = await hl.fetchMarket('ETH');
  const t = await hl.fetchTicker('ETH');
  // Rest deep below mid (integer px is always valid); size to clear the $10 min notional.
  const px = String(Math.max(1, Math.floor(t.last / 2)));
  const lot = Number(m.amountIncrement);
  const needed = (10 * 1.2) / Number(px);
  const amount = (Math.ceil(needed / lot) * lot).toFixed(Math.max(0, (m.amountIncrement.split('.')[1] || '').length));
  const order = await hl.createOrder({ symbol: 'ETH', side: 'buy', type: 'limit', amount, price: px });
  const open = await hl.fetchOpenOrders('ETH');
  await hl.cancelOrder(order.id, 'ETH');
  const after = await hl.fetchOpenOrders('ETH');
  return {
    ...out,
    order: { ok: !!order.id, id: order.id, status: order.status },
    open: { ok: open.some((o) => o.id === order.id), count: open.length },
    canceled: { ok: !after.some((o) => o.id === order.id) },
  };
}
`;

export default function (data: VenueSpecContext) {
  const pub = runVenueAction("venues/hyperliquid/public", data.usageKeyHeaders, BUNDLE, PUBLIC_MAIN, null);
  if (pub) {
    const ticker = pub.ticker as { ok: boolean; httpStatus?: number };
    // Egress-posture measurement (plan D8): a 451/403 here means the venue
    // started IP-gating our region → flip the connector docs to proxy-required.
    console.log(`[venues/hyperliquid] direct-egress measurement: ${JSON.stringify(pub.ticker)}`);
    checkAndLog(null, {
      "hyperliquid public ticker via direct CVM egress": () => ticker.ok === true,
      "hyperliquid market rules derived from szDecimals": () => (pub.market as { ok: boolean }).ok === true,
      "hyperliquid funding rate (perp surface)": () => (pub.funding as { ok: boolean }).ok === true,
      "hyperliquid bad symbol maps to bad_symbol": () => (pub.badSymbol as { ok: boolean }).ok === true,
    }, "venues/hyperliquid/public");
  }

  const key = __ENV.HYPERLIQUID_TESTNET_KEY;
  if (!key) {
    console.warn(
      "venues/hyperliquid: HYPERLIQUID_TESTNET_KEY not set — skipping authenticated lifecycle (M2.5 gate needs it)",
    );
    return;
  }
  const lifecycle = runVenueAction(
    "venues/hyperliquid/lifecycle",
    data.usageKeyHeaders,
    BUNDLE,
    LIFECYCLE_MAIN,
    { key, accountAddress: __ENV.HYPERLIQUID_ACCOUNT_ADDRESS ?? "" },
  );
  if (lifecycle) {
    checkAndLog(null, {
      "testnet balances read": () => (lifecycle.balances as { ok: boolean }).ok === true,
      "testnet limit order rested": () => (lifecycle.order as { ok: boolean }).ok === true,
      "testnet order listed in openOrders": () => (lifecycle.open as { ok: boolean }).ok === true,
      "testnet order canceled": () => (lifecycle.canceled as { ok: boolean }).ok === true,
    }, "venues/hyperliquid/lifecycle");
  }
}

export const handleSummary = warnOnHttpFailures;
