/**
 * lit-venues M0 spike (plans/ccxt-venue-layer-and-email-approval.md, M0 gate).
 *
 * Executes the lit-venues IIFE bundle INSIDE a Lit Action and:
 *   1. fetches a public Coinbase ticker (geo-safe) — proves the bundle parses,
 *      runs, and can sign-free round-trip through the action fetch;
 *   2. probes the Binance spot testnet — this is the egress-geography
 *      measurement, not a pass/fail: HTTP 451 ⇒ US egress ⇒ M1 needs the D4
 *      proxy (the probe result is logged either way).
 *
 * Uses plain @playwright/test (not fixtures/test) so it runs against any
 * API_BASE_URL without the local-Anvil snapshot fixture.
 */

import { test, expect } from '@playwright/test';
import { LitApiClient } from '../../fixtures/api-client';
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const BUNDLE_PATH = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../../lit-venues/dist/lit-venues.iife.js',
);

test.describe('lit-venues — M0 spike', () => {
  test('IIFE bundle runs inside a Lit Action and fetches a public ticker', async () => {
    test.skip(!existsSync(BUNDLE_PATH), 'lit-venues bundle missing — run `npm run build` in lit-venues/');

    const bundle = readFileSync(BUNDLE_PATH, 'utf8');
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

    const apiClient = new LitApiClient();
    const stamp = Date.now();
    const account = await apiClient.newAccount({
      account_name: `e2e-venues-${stamp}`,
      account_description: 'lit-venues M0 spike',
    });
    const { usage_api_key: usageApiKey } = await apiClient.addUsageApiKey(account.api_key, {
      name: `e2e-venues-${stamp}-usage`,
      execute_in_groups: [0],
    });

    const result = await apiClient.litAction(usageApiKey, { code });
    expect(result.has_error, `action errored; logs: ${result.logs ?? '<none>'}`).toBe(false);

    const payload = result.response as {
      version: string;
      coinbaseLast: number;
      binanceTestnet: { ok: boolean; code?: string; httpStatus?: number; message?: string };
    };
    expect(payload.version).toBe('0.1.0');
    expect(payload.coinbaseLast).toBeGreaterThan(0);
    expect(payload.binanceTestnet).toBeDefined();

    // Egress-geography measurement (M0 deliverable) — recorded, not asserted.
    console.log(
      `[lit-venues spike] bundle=${Buffer.byteLength(bundle)}B coinbase BTC/USD=${payload.coinbaseLast} ` +
        `binanceTestnet=${JSON.stringify(payload.binanceTestnet)}`,
    );
  });
});
