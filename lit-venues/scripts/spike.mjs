/**
 * M0 spike runner — same flow as e2e/tests/api/lit-venues-spike.spec.ts but
 * standalone (plain node, no Playwright) so it can be pointed at any
 * environment quickly:
 *
 *   LIT_API_BASE_URL=https://test.chipotle.litprotocol.com node scripts/spike.mjs
 *
 * Provisions a throwaway account, executes the IIFE bundle inside a Lit
 * Action, fetches a public Coinbase ticker, and probes Binance spot testnet
 * (the egress-geography measurement: HTTP 451 ⇒ US egress ⇒ plan D4 proxy).
 */

import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

// Optional: ENV_FILE=/path/to/.env — loads LIT_* vars (e.g. the flows repo's
// .env, whose LIT_ADMIN_API_KEY/LIT_API_KEY pair with its LIT_API_BASE_URL).
// Values are never printed.
if (process.env.ENV_FILE && existsSync(process.env.ENV_FILE)) {
  for (const line of readFileSync(process.env.ENV_FILE, 'utf8').split('\n')) {
    const m = line.match(/^(LIT_[A-Z0-9_]+)=("?)(.*)\2\s*$/);
    if (m && process.env[m[1]] === undefined) process.env[m[1]] = m[3];
  }
}

const BASE = (process.env.LIT_API_BASE_URL ?? 'https://test.chipotle.litprotocol.com').replace(/\/$/, '');
const BUNDLE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../dist/lit-venues.iife.js');

async function api(method, p, { apiKey, body } = {}) {
  const res = await fetch(`${BASE}/core/v1${p}`, {
    method,
    headers: {
      ...(body !== undefined ? { 'content-type': 'application/json' } : {}),
      ...(apiKey ? { 'x-api-key': apiKey } : {}),
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${method} ${p} → ${res.status}: ${text.slice(0, 500)}`);
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

const bundle = readFileSync(BUNDLE, 'utf8');
const code = `${bundle}
async function main() {
  const coinbase = LitVenues.createVenue({ venueId: 'coinbase' });
  const ticker = await coinbase.fetchTicker('BTC/USD');

  let binanceTestnet;
  try {
    const binance = LitVenues.createVenue({ venueId: 'binance', sandbox: true });
    const t = await binance.fetchTicker('BTC/USDT');
    binanceTestnet = { ok: true, last: t.last };
  } catch (e) {
    binanceTestnet = {
      ok: false,
      code: e && e.code,
      httpStatus: e && e.httpStatus,
      message: String((e && e.message) || e).slice(0, 200),
    };
  }

  return { version: LitVenues.VERSION, coinbaseLast: ticker.last, binanceTestnet };
}
`;

console.log(`[spike] target=${BASE} bundle=${Buffer.byteLength(bundle)}B`);
const stamp = Date.now();

// Key resolution: existing usage key > admin-minted usage key > fresh account
// (the last needs an env with open/zero-cost management calls).
let usageKey = process.env.LIT_USAGE_API_KEY;
if (usageKey) {
  console.log('[spike] using usage key from env');
} else {
  let adminKey = process.env.LIT_ADMIN_API_KEY;
  if (adminKey) {
    console.log('[spike] using admin key from env to mint a usage key');
  } else {
    const account = await api('POST', '/new_account', {
      body: { account_name: `lit-venues-spike-${stamp}`, account_description: 'lit-venues M0 spike (throwaway)' },
    });
    console.log(`[spike] account created (wallet ${account.wallet_address})`);
    adminKey = account.api_key;
  }
  const groups = [0];
  if (process.env.LIT_GROUP_ID && !Number.isNaN(Number(process.env.LIT_GROUP_ID))) {
    groups.push(Number(process.env.LIT_GROUP_ID));
  }
  ({ usage_api_key: usageKey } = await api('POST', '/add_usage_api_key', {
    apiKey: adminKey,
    body: {
      name: `lit-venues-spike-${stamp}-usage`,
      description: 'lit-venues M0 spike usage key',
      can_create_groups: false,
      can_delete_groups: false,
      can_create_pkps: false,
      manage_ipfs_ids_in_groups: [],
      add_pkp_to_groups: [],
      remove_pkp_from_groups: [],
      execute_in_groups: groups,
    },
  }));
}

const t0 = Date.now();
const result = await api('POST', '/lit_action', { apiKey: usageKey, body: { code, js_params: null } });
const elapsed = Date.now() - t0;

console.log(`[spike] executed in ${elapsed}ms has_error=${result.has_error}`);
if (result.logs) console.log(`[spike] logs:\n${result.logs}`);
console.log('[spike] response:', JSON.stringify(result.response, null, 2));

const payload = result.response ?? {};
const pass = !result.has_error && payload.version === '0.1.0' && Number(payload.coinbaseLast) > 0;
console.log(pass ? '[spike] M0 GATE: PASS' : '[spike] M0 GATE: FAIL');
if (payload.binanceTestnet && payload.binanceTestnet.ok === false) {
  console.log(
    payload.binanceTestnet.httpStatus === 451
      ? '[spike] egress measurement: US-region egress (binance 451) — M1 Binance gate needs the D4 proxy or binanceus'
      : '[spike] egress measurement: binance testnet unreachable for a non-geo reason — inspect above',
  );
} else if (payload.binanceTestnet?.ok) {
  console.log('[spike] egress measurement: binance testnet reachable — egress is NOT US-blocked');
}
process.exit(pass ? 0 : 1);
