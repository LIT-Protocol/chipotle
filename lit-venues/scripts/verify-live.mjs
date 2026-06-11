/**
 * Live conformance for the lit-venues connectors against REAL exchange APIs.
 * Runs the actual connector code (request building, signing, parsing, error
 * mapping) over the network — the step above unit-mocks and below in-TEE.
 *
 *   - Binance goes through an egress proxy (Binance geo-blocks US IPs); set
 *     LIT_VENUES_PROXY=http://user:pass@host:port (or it's read from
 *     /tmp/proxy_url.txt when present).
 *   - Coinbase Advanced Trade is reached directly (US-reachable).
 *   - Hyperliquid is reached directly by default (no exchange-side IP
 *     allowlist; geo posture is measured by this script — plan D8). Set
 *     HYPERLIQUID_VIA_PROXY=true to route it through LIT_VENUES_PROXY too.
 *   - Public endpoints always run. Authenticated endpoints run only when keys
 *     are supplied:
 *       BINANCE_KEY / BINANCE_SECRET [/ BINANCE_KEY_TYPE=hmac|ed25519]
 *       COINBASE_KEY_NAME / COINBASE_PRIVATE_KEY
 *       HYPERLIQUID_PRIVATE_KEY [/ HYPERLIQUID_ACCOUNT_ADDRESS for agent mode]
 *         [/ HYPERLIQUID_SANDBOX=false to hit mainnet — default is testnet]
 *
 * undici provides the proxy-capable fetch (the library stays dependency-free);
 * in the TEE this same role is played by Lit.Actions.proxiedFetch.
 *
 *   node scripts/verify-live.mjs
 */

import { readFileSync, existsSync } from 'node:fs';
import { fetch as uFetch, ProxyAgent } from 'undici';
import { createVenue } from '../dist/lit-venues.mjs';

const PROXY =
  process.env.LIT_VENUES_PROXY ||
  (existsSync('/tmp/proxy_url.txt') ? readFileSync('/tmp/proxy_url.txt', 'utf8').trim() : '');

/** Build a lit-venues FetchLike. With a proxy URL it routes via undici ProxyAgent. */
function nodeFetch(proxyUrl) {
  const dispatcher = proxyUrl ? new ProxyAgent(proxyUrl) : undefined;
  return async (url, init = {}) => {
    const res = await uFetch(url, dispatcher ? { ...init, dispatcher } : init);
    return { status: res.status, ok: res.ok, text: () => res.text() };
  };
}

let pass = 0;
let fail = 0;
async function check(label, fn) {
  try {
    const detail = await fn();
    pass++;
    console.log(`  ✓ ${label}${detail ? ` — ${detail}` : ''}`);
  } catch (e) {
    fail++;
    console.log(`  ✗ ${label} — ${e?.code ? `[${e.code}] ` : ''}${e?.message ?? e}`);
  }
}

console.log('\n== Binance (via egress proxy) ==');
if (!PROXY) {
  console.log('  ! no proxy configured (LIT_VENUES_PROXY) — skipping Binance (US IPs are geo-blocked)');
} else {
  const binance = createVenue({ venueId: 'binance', fetchImpl: nodeFetch(PROXY) });
  await check('fetchTicker BTC/USDT', async () => {
    const t = await binance.fetchTicker('BTC/USDT');
    if (!(t.last > 0)) throw new Error(`bad last: ${t.last}`);
    return `last=${t.last}`;
  });
  await check('fetchMarket BTC/USDT (tick/lot rules)', async () => {
    const m = await binance.fetchMarket('BTC/USDT');
    if (!m.priceIncrement || !m.amountIncrement) throw new Error('missing increments');
    return `tick=${m.priceIncrement} lot=${m.amountIncrement}`;
  });
  await check('bad symbol maps to bad_symbol', async () => {
    try {
      await binance.fetchTicker('NOTACOIN/USDT');
      throw new Error('expected an error');
    } catch (e) {
      if (e?.code !== 'bad_symbol') throw new Error(`got code ${e?.code}`);
      return 'rejected as expected';
    }
  });
  if (process.env.BINANCE_KEY && process.env.BINANCE_SECRET) {
    const authed = createVenue({
      venueId: 'binance',
      sandbox: process.env.BINANCE_SANDBOX !== 'false',
      credentials: {
        apiKey: process.env.BINANCE_KEY,
        secret: process.env.BINANCE_SECRET,
        keyType: process.env.BINANCE_KEY_TYPE ?? 'hmac',
      },
      fetchImpl: nodeFetch(PROXY),
    });
    await check('fetchBalances (authenticated)', async () => {
      const b = await authed.fetchBalances();
      return `${b.length} non-zero assets`;
    });
  } else {
    console.log('  ! no BINANCE_KEY/SECRET — skipping authenticated (balances, orders)');
  }
}

console.log('\n== Coinbase Advanced Trade (direct) ==');
{
  const coinbase = createVenue({ venueId: 'coinbase', fetchImpl: nodeFetch('') });
  await check('fetchTicker BTC/USD', async () => {
    const t = await coinbase.fetchTicker('BTC/USD');
    if (!(t.last > 0)) throw new Error(`bad last: ${t.last}`);
    return `last=${t.last}`;
  });
  await check('fetchMarket ETH/USD (tick/lot rules)', async () => {
    const m = await coinbase.fetchMarket('ETH/USD');
    if (!m.priceIncrement || !m.amountIncrement) throw new Error('missing increments');
    return `tick=${m.priceIncrement} lot=${m.amountIncrement} base=${m.base}`;
  });
  await check('bad symbol maps to an error', async () => {
    try {
      await coinbase.fetchTicker('NOTACOIN/USD');
      throw new Error('expected an error');
    } catch (e) {
      if (!e?.code) throw new Error('no VenueError code');
      return `rejected (${e.code})`;
    }
  });
  if (process.env.COINBASE_KEY_NAME && process.env.COINBASE_PRIVATE_KEY) {
    const authed = createVenue({
      venueId: 'coinbase',
      credentials: { apiKey: process.env.COINBASE_KEY_NAME, secret: process.env.COINBASE_PRIVATE_KEY },
      fetchImpl: nodeFetch(''),
    });
    await check('fetchBalances (authenticated, ES256 JWT)', async () => {
      const b = await authed.fetchBalances();
      return `${b.length} non-zero accounts`;
    });
  } else {
    console.log('  ! no COINBASE_KEY_NAME/PRIVATE_KEY — skipping authenticated (balances, orders)');
  }
}

console.log('\n== Hyperliquid (PKP-native, direct) ==');
{
  const hlFetch = nodeFetch(process.env.HYPERLIQUID_VIA_PROXY === 'true' ? PROXY : '');
  const hl = createVenue({ venueId: 'hyperliquid', fetchImpl: hlFetch });
  await check('fetchTicker BTC (geo-posture measurement: direct egress)', async () => {
    const t = await hl.fetchTicker('BTC');
    if (!(t.last > 0)) throw new Error(`bad last: ${t.last}`);
    return `last=${t.last}`;
  });
  await check('fetchMarket ETH (szDecimals → tick/lot rules)', async () => {
    const m = await hl.fetchMarket('ETH');
    if (!m.priceIncrement || !m.amountIncrement) throw new Error('missing increments');
    return `pxMaxStep=${m.priceIncrement} lot=${m.amountIncrement} maxLev=${m.info?.maxLeverage}`;
  });
  await check('fetchFundingRate BTC (perp surface)', async () => {
    const fr = await hl.fetchFundingRate('BTC');
    if (fr.fundingRate === undefined) throw new Error('no funding');
    return `funding=${fr.fundingRate} mark=${fr.markPrice}`;
  });
  await check('bad symbol maps to bad_symbol', async () => {
    try {
      await hl.fetchTicker('NOTACOIN');
      throw new Error('expected an error');
    } catch (e) {
      if (e?.code !== 'bad_symbol') throw new Error(`got code ${e?.code}`);
      return 'rejected as expected';
    }
  });
  if (process.env.HYPERLIQUID_PRIVATE_KEY) {
    const sandbox = process.env.HYPERLIQUID_SANDBOX !== 'false';
    const authed = createVenue({
      venueId: 'hyperliquid',
      sandbox,
      credentials: {
        keyType: 'pkp-eip712',
        privateKey: process.env.HYPERLIQUID_PRIVATE_KEY,
        accountAddress: process.env.HYPERLIQUID_ACCOUNT_ADDRESS,
      },
      fetchImpl: hlFetch,
    });
    await check(`fetchBalances (${sandbox ? 'testnet' : 'MAINNET'})`, async () => {
      const b = await authed.fetchBalances();
      return `USDC free=${b[0]?.free} total=${b[0]?.total}`;
    });
    await check('order lifecycle: place deep GTC limit, list, cancel', async () => {
      const m = await authed.fetchMarket('ETH');
      const t = await authed.fetchTicker('ETH');
      // bid 50% under mid so it rests without filling; integer px is always valid
      const px = String(Math.max(1, Math.floor(t.last / 2)));
      const order = await authed.createOrder({ symbol: 'ETH', side: 'buy', type: 'limit', amount: m.minAmount ?? '0.01', price: px });
      const open = await authed.fetchOpenOrders('ETH');
      await authed.cancelOrder(order.id, 'ETH');
      return `oid=${order.id} rested (${open.length} open) then canceled`;
    });
  } else {
    console.log('  ! no HYPERLIQUID_PRIVATE_KEY — skipping authenticated (balances, order lifecycle)');
  }
}

console.log(`\n${fail === 0 ? 'LIVE CONFORMANCE: PASS' : 'LIVE CONFORMANCE: FAIL'} (${pass} passed, ${fail} failed)\n`);
process.exit(fail === 0 ? 0 : 1);
