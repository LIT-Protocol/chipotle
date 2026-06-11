import { describe, expect, it } from 'vitest';
import { p256 } from '@noble/curves/p256';
import { sha256 } from '@noble/hashes/sha256';
import { utf8ToBytes } from '@noble/hashes/utils';
import { CoinbaseClient } from '../src/venues/coinbase';

function mockFetch(queue: Array<{ status?: number; body: unknown }>) {
  const calls: Array<{ url: string; init?: Record<string, unknown> }> = [];
  const f = async (url: string, init?: Record<string, unknown>) => {
    calls.push({ url, init });
    const next = queue.shift() ?? { status: 200, body: {} };
    const status = next.status ?? 200;
    return {
      status,
      ok: status >= 200 && status < 300,
      text: async () => (typeof next.body === 'string' ? next.body : JSON.stringify(next.body)),
    };
  };
  return { f, calls };
}

const scalarHex = '0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20';
const KEY_NAME = 'organizations/abc/apiKeys/def';
const CREDS = { apiKey: KEY_NAME, secret: scalarHex, keyType: 'es256-jwt' as const };

describe('CoinbaseClient', () => {
  it('fetchTicker hits the public market endpoint with no auth header', async () => {
    const { f, calls } = mockFetch([{ body: { price: '2500.25' } }]);
    const t = await new CoinbaseClient({ venueId: 'coinbase', fetchImpl: f }).fetchTicker('ETH/USD');
    expect(calls[0]!.url).toBe('https://api.coinbase.com/api/v3/brokerage/market/products/ETH-USD');
    expect((calls[0]!.init?.headers as Record<string, string>).Authorization).toBeUndefined();
    expect(t.last).toBe(2500.25);
  });

  it('authenticated calls carry a verifiable ES256 JWT with the query-less uri claim', async () => {
    const { f, calls } = mockFetch([{ body: { accounts: [] } }]);
    await new CoinbaseClient({
      venueId: 'coinbase',
      credentials: CREDS,
      fetchImpl: f,
      nowMs: () => 1_700_000_000_000,
    }).fetchBalances();

    expect(calls[0]!.url).toBe('https://api.coinbase.com/api/v3/brokerage/accounts?limit=250');
    const auth = (calls[0]!.init?.headers as Record<string, string>).Authorization!;
    expect(auth.startsWith('Bearer ')).toBe(true);
    const [h, p, s] = auth.slice('Bearer '.length).split('.') as [string, string, string];
    const header = JSON.parse(Buffer.from(h, 'base64url').toString());
    expect(header.alg).toBe('ES256');
    expect(header.kid).toBe(KEY_NAME);
    const payload = JSON.parse(Buffer.from(p, 'base64url').toString());
    expect(payload).toMatchObject({
      iss: 'cdp',
      sub: KEY_NAME,
      uri: 'GET api.coinbase.com/api/v3/brokerage/accounts',
      nbf: 1_700_000_000,
      exp: 1_700_000_120,
    });
    const pub = p256.getPublicKey(Uint8Array.from(Buffer.from(scalarHex, 'hex')));
    expect(
      p256.verify(new Uint8Array(Buffer.from(s, 'base64url')), sha256(utf8ToBytes(`${h}.${p}`)), pub),
    ).toBe(true);
  });

  it('builds limit and market order bodies in Advanced Trade shape', async () => {
    const ok = { body: { success: true, success_response: { order_id: 'ord-1' } } };
    const { f, calls } = mockFetch([ok, ok]);
    const client = new CoinbaseClient({ venueId: 'coinbase', credentials: CREDS, fetchImpl: f });

    const limit = await client.createOrder({
      symbol: 'ETH/USD',
      side: 'buy',
      type: 'limit',
      amount: '0.5',
      price: '1000',
      clientOrderId: 'coid-1',
    });
    expect(JSON.parse(calls[0]!.init?.body as string)).toEqual({
      client_order_id: 'coid-1',
      product_id: 'ETH-USD',
      side: 'BUY',
      order_configuration: { limit_limit_gtc: { base_size: '0.5', limit_price: '1000' } },
    });
    expect(limit).toMatchObject({ id: 'ord-1', status: 'open', clientOrderId: 'coid-1' });

    await client.createOrder({ symbol: 'ETH/USD', side: 'sell', type: 'market', amount: '0.25' });
    expect(JSON.parse(calls[1]!.init?.body as string).order_configuration).toEqual({
      market_market_ioc: { base_size: '0.25' },
    });

    await expect(
      client.createOrder({ symbol: 'ETH/USD', side: 'buy', type: 'market', amount: '1' }),
    ).rejects.toMatchObject({ code: 'invalid_request' });
  });

  it('maps success:false order responses to the error taxonomy', async () => {
    const { f } = mockFetch([
      { body: { success: false, error_response: { error: 'INSUFFICIENT_FUND', message: 'Insufficient balance in source account' } } },
    ]);
    await expect(
      new CoinbaseClient({ venueId: 'coinbase', credentials: CREDS, fetchImpl: f }).createOrder({
        symbol: 'ETH/USD',
        side: 'buy',
        type: 'limit',
        amount: '999',
        price: '1000',
      }),
    ).rejects.toMatchObject({ code: 'insufficient_funds', venueCode: 'INSUFFICIENT_FUND' });
  });

  it('cancelOrder uses batch_cancel and surfaces failure reasons', async () => {
    const { f, calls } = mockFetch([{ body: { results: [{ success: true }] } }]);
    const client = new CoinbaseClient({ venueId: 'coinbase', credentials: CREDS, fetchImpl: f });
    await client.cancelOrder('ord-9', 'ETH/USD');
    expect(calls[0]!.url).toBe('https://api.coinbase.com/api/v3/brokerage/orders/batch_cancel');
    expect(JSON.parse(calls[0]!.init?.body as string)).toEqual({ order_ids: ['ord-9'] });

    const failing = mockFetch([{ body: { results: [{ success: false, failure_reason: 'UNKNOWN_CANCEL_ORDER' }] } }]);
    await expect(
      new CoinbaseClient({ venueId: 'coinbase', credentials: CREDS, fetchImpl: failing.f }).cancelOrder('x', 'ETH/USD'),
    ).rejects.toMatchObject({ code: 'invalid_request' });
  });

  it('maps open orders including type, status, and unified symbol', async () => {
    const { f, calls } = mockFetch([
      {
        body: {
          orders: [
            {
              order_id: 'o1',
              client_order_id: 'c1',
              product_id: 'ETH-USD',
              side: 'SELL',
              status: 'OPEN',
              filled_size: '0.1',
              created_time: '2026-06-10T00:00:00Z',
              order_configuration: { limit_limit_gtc: { base_size: '1', limit_price: '9000' } },
            },
          ],
        },
      },
    ]);
    const orders = await new CoinbaseClient({ venueId: 'coinbase', credentials: CREDS, fetchImpl: f }).fetchOpenOrders('ETH/USD');
    expect(calls[0]!.url).toContain('order_status=OPEN');
    expect(calls[0]!.url).toContain('product_id=ETH-USD');
    expect(orders[0]).toMatchObject({
      id: 'o1',
      clientOrderId: 'c1',
      symbol: 'ETH/USD',
      side: 'sell',
      type: 'limit',
      status: 'open',
      price: '9000',
      amount: '1',
      filled: '0.1',
    });
  });

  it('rejects sandbox mode honestly (Advanced Trade has none)', () => {
    expect(() => new CoinbaseClient({ venueId: 'coinbase', sandbox: true })).toThrow(/no functional sandbox/);
  });
});
