import { describe, expect, it } from 'vitest';
import { ed25519 } from '@noble/curves/ed25519';
import { utf8ToBytes } from '@noble/hashes/utils';
import { BinanceClient } from '../src/venues/binance';
import { b64decode } from '../src/signing';

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

const VECTOR_KEY = 'vmPUZE6mv9SD5VNHk4HlWFsOr6aKE2zvsw0MuIgwCIPy6utIco14y7Ju91duEh8A';
const VECTOR_SECRET = 'NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j';
const VECTOR_SIG = 'c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71';

describe('BinanceClient', () => {
  it('builds a createOrder request whose signature matches the Binance docs vector byte-for-byte', async () => {
    const { f, calls } = mockFetch([
      { body: { orderId: 12345, status: 'NEW', side: 'BUY', type: 'LIMIT', origQty: '1', executedQty: '0', price: '0.1', transactTime: 1499827319559 } },
    ]);
    const client = new BinanceClient({
      venueId: 'binance',
      credentials: { apiKey: VECTOR_KEY, secret: VECTOR_SECRET, keyType: 'hmac' },
      fetchImpl: f,
      nowMs: () => 1499827319559,
    });
    const order = await client.createOrder({ symbol: 'LTC/BTC', side: 'buy', type: 'limit', amount: '1', price: '0.1' });
    expect(calls[0]!.url).toBe(
      'https://api.binance.com/api/v3/order' +
        '?symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1' +
        `&recvWindow=5000&timestamp=1499827319559&signature=${VECTOR_SIG}`,
    );
    expect(calls[0]!.init?.method).toBe('POST');
    expect((calls[0]!.init?.headers as Record<string, string>)['X-MBX-APIKEY']).toBe(VECTOR_KEY);
    expect(order).toMatchObject({ id: '12345', status: 'open', side: 'buy', symbol: 'LTC/BTC' });
  });

  it('signs with Ed25519 keys when keyType is ed25519 (verifiable signature over the exact query payload)', async () => {
    const seedHex = '4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb';
    const { f, calls } = mockFetch([{ body: { balances: [] } }]);
    const client = new BinanceClient({
      venueId: 'binance',
      credentials: { apiKey: 'k', secret: seedHex, keyType: 'ed25519' },
      fetchImpl: f,
      nowMs: () => 1700000000000,
    });
    await client.fetchBalances();
    const url = new URL(calls[0]!.url);
    const sig = url.searchParams.get('signature')!;
    url.searchParams.delete('signature');
    const payload = url.search.slice(1);
    expect(payload).toBe('recvWindow=5000&timestamp=1700000000000');
    const pub = ed25519.getPublicKey(Uint8Array.from(Buffer.from(seedHex, 'hex')));
    expect(ed25519.verify(b64decode(sig), utf8ToBytes(payload), pub)).toBe(true);
  });

  it('routes sandbox to testnet.binance.vision and binanceus to api.binance.us', async () => {
    const a = mockFetch([{ body: { price: '50000' } }]);
    await new BinanceClient({ venueId: 'binance', sandbox: true, fetchImpl: a.f }).fetchTicker('BTC/USDT');
    expect(a.calls[0]!.url).toBe('https://testnet.binance.vision/api/v3/ticker/price?symbol=BTCUSDT');
    expect(a.calls[0]!.init?.headers).toEqual({});

    const b = mockFetch([{ body: { price: '50000' } }]);
    await new BinanceClient({ venueId: 'binanceus', fetchImpl: b.f }).fetchTicker('BTC/USD');
    expect(b.calls[0]!.url).toBe('https://api.binance.us/api/v3/ticker/price?symbol=BTCUSD');
    expect(() => new BinanceClient({ venueId: 'binanceus', sandbox: true })).toThrow(/no public testnet/);
  });

  it('maps venue errors to the unified taxonomy', async () => {
    const bad = mockFetch([{ status: 400, body: { code: -1121, msg: 'Invalid symbol.' } }]);
    await expect(
      new BinanceClient({ venueId: 'binance', fetchImpl: bad.f }).fetchTicker('NOPE/NOPE'),
    ).rejects.toMatchObject({ name: 'VenueError', code: 'bad_symbol', venueCode: -1121 });

    const geo = mockFetch([{ status: 451, body: 'Unavailable For Legal Reasons' }]);
    await expect(
      new BinanceClient({ venueId: 'binance', fetchImpl: geo.f }).fetchTicker('BTC/USDT'),
    ).rejects.toMatchObject({ code: 'venue_unavailable', httpStatus: 451 });

    const funds = mockFetch([{ status: 400, body: { code: -2010, msg: 'Account has insufficient balance' } }]);
    await expect(
      new BinanceClient({
        venueId: 'binance',
        credentials: { apiKey: 'k', secret: 's' },
        fetchImpl: funds.f,
      }).createOrder({ symbol: 'BTC/USDT', side: 'buy', type: 'limit', amount: '1', price: '1' }),
    ).rejects.toMatchObject({ code: 'insufficient_funds' });
  });

  it('filters zero balances and sums free+locked exactly', async () => {
    const { f } = mockFetch([
      {
        body: {
          balances: [
            { asset: 'BTC', free: '0.1', locked: '0.2' },
            { asset: 'ETH', free: '0.00000000', locked: '0.00000000' },
          ],
        },
      },
    ]);
    const balances = await new BinanceClient({
      venueId: 'binance',
      credentials: { apiKey: 'k', secret: 's' },
      fetchImpl: f,
    }).fetchBalances();
    expect(balances).toEqual([{ asset: 'BTC', free: '0.1', total: '0.3' }]);
  });

});
