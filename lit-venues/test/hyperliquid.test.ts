import { describe, expect, it } from 'vitest';
import { HyperliquidClient } from '../src/venues/hyperliquid';
import { privateKeyToAddress } from '../src/eip712';

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

const KEY = '0x0123456789012345678901234567890123456789012345678901234567890123';
const META = {
  universe: [
    { name: 'BTC', szDecimals: 5, maxLeverage: 40 },
    { name: 'ETH', szDecimals: 4, maxLeverage: 25 },
  ],
};

function body(call: { init?: Record<string, unknown> }): Record<string, unknown> {
  return JSON.parse(String(call.init?.body)) as Record<string, unknown>;
}

describe('HyperliquidClient', () => {
  it('fetches a ticker from allMids', async () => {
    const { f, calls } = mockFetch([{ body: { ETH: '1670.05' } }]);
    const t = await new HyperliquidClient({ venueId: 'hyperliquid', fetchImpl: f }).fetchTicker('ETH/USDC:USDC');
    expect(calls[0]!.url).toBe('https://api.hyperliquid.xyz/info');
    expect(body(calls[0]!)).toEqual({ type: 'allMids' });
    expect(t.last).toBe(1670.05);
  });

  it('routes sandbox to the testnet API', async () => {
    const { f, calls } = mockFetch([{ body: { ETH: '1' } }]);
    await new HyperliquidClient({ venueId: 'hyperliquid', sandbox: true, fetchImpl: f }).fetchTicker('ETH');
    expect(calls[0]!.url).toBe('https://api.hyperliquid-testnet.xyz/info');
  });

  it('derives market rules from meta (MAX_DECIMALS − szDecimals)', async () => {
    const { f } = mockFetch([{ body: META }]);
    const m = await new HyperliquidClient({ venueId: 'hyperliquid', fetchImpl: f }).fetchMarket('ETH');
    expect(m).toMatchObject({
      base: 'ETH',
      quote: 'USDC',
      priceIncrement: '0.01',
      amountIncrement: '0.0001',
      minNotional: '10',
    });
    expect((m.info as { assetId: number }).assetId).toBe(1);
  });

  it('answers fetchMarket from the injected markets cache without HTTP (M1 markets-cache injection)', async () => {
    const { f, calls } = mockFetch([]);
    const cached = {
      symbol: 'ETH',
      base: 'ETH',
      quote: 'USDC',
      priceIncrement: '0.01',
      amountIncrement: '0.0001',
    };
    const m = await new HyperliquidClient({
      venueId: 'hyperliquid',
      fetchImpl: f,
      markets: { ETH: cached },
    }).fetchMarket('ETH');
    expect(m).toBe(cached);
    expect(calls).toHaveLength(0);
  });

  it('places a limit order: exact wire action, signed, nonce = nowMs', async () => {
    const { f, calls } = mockFetch([
      { body: META },
      { body: { status: 'ok', response: { type: 'order', data: { statuses: [{ resting: { oid: 77738308 } }] } } } },
    ]);
    const client = new HyperliquidClient({
      venueId: 'hyperliquid',
      credentials: { keyType: 'pkp-eip712', privateKey: KEY },
      fetchImpl: f,
      nowMs: () => 1677777606040,
    });
    const order = await client.createOrder({
      symbol: 'ETH',
      side: 'buy',
      type: 'limit',
      amount: '0.0147',
      price: '1670.10',
      timeInForce: 'IOC',
    });
    expect(calls[1]!.url).toBe('https://api.hyperliquid.xyz/exchange');
    const sent = body(calls[1]!);
    expect(sent.action).toEqual({
      type: 'order',
      orders: [{ a: 1, b: true, p: '1670.1', s: '0.0147', r: false, t: { limit: { tif: 'Ioc' } } }],
      grouping: 'na',
    });
    expect(sent.nonce).toBe(1677777606040);
    const sig = sent.signature as { r: string; s: string; v: number };
    expect(sig.r).toMatch(/^0x[0-9a-f]+$/);
    expect([27, 28]).toContain(sig.v);
    expect(order).toMatchObject({ id: '77738308', status: 'open', price: '1670.1', amount: '0.0147' });
  });

  it('market orders become aggressive IOC limits priced off mid ± slippage', async () => {
    const { f, calls } = mockFetch([
      { body: META },
      { body: { ETH: '2000.0' } },
      { body: { status: 'ok', response: { type: 'order', data: { statuses: [{ filled: { oid: 1, totalSz: '0.5', avgPx: '2001.3' } }] } } } },
    ]);
    const client = new HyperliquidClient({
      venueId: 'hyperliquid',
      credentials: { privateKey: KEY },
      fetchImpl: f,
    });
    const order = await client.createOrder({ symbol: 'ETH', side: 'buy', type: 'market', amount: '0.5' });
    const sent = body(calls[2]!);
    const wire = (sent.action as { orders: Array<Record<string, unknown>> }).orders[0]!;
    expect(wire.p).toBe('2100'); // 2000 × 1.05, default 500 bps
    expect(wire.t).toEqual({ limit: { tif: 'Ioc' } });
    expect(order).toMatchObject({ status: 'filled', filled: '0.5', price: '2001.3' });
  });

  it('attaches the builder code when configured', async () => {
    const { f, calls } = mockFetch([
      { body: META },
      { body: { status: 'ok', response: { type: 'order', data: { statuses: [{ resting: { oid: 2 } }] } } } },
    ]);
    await new HyperliquidClient({
      venueId: 'hyperliquid',
      credentials: { privateKey: KEY },
      builder: { address: '0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA', feeTenthBps: 10 },
      fetchImpl: f,
    }).createOrder({ symbol: 'ETH', side: 'buy', type: 'limit', amount: '1', price: '2000' });
    expect((body(calls[1]!).action as Record<string, unknown>).builder).toEqual({
      b: '0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      f: 10,
    });
  });

  it('enforces the precision rules before signing', async () => {
    const meta2 = { body: META };
    const c = (queue: Array<{ status?: number; body: unknown }>) =>
      new HyperliquidClient({ venueId: 'hyperliquid', credentials: { privateKey: KEY }, fetchImpl: mockFetch(queue).f });
    // szDecimals: ETH allows 4 decimals of size
    await expect(
      c([meta2]).createOrder({ symbol: 'ETH', side: 'buy', type: 'limit', amount: '0.00001', price: '2000' }),
    ).rejects.toMatchObject({ code: 'invalid_request' });
    // 6 − szDecimals = 2 price decimals for ETH
    await expect(
      c([{ body: META }]).createOrder({ symbol: 'ETH', side: 'buy', type: 'limit', amount: '1', price: '2000.123' }),
    ).rejects.toMatchObject({ code: 'invalid_request' });
    // > 5 significant figures on a fractional price
    await expect(
      c([{ body: META }]).createOrder({ symbol: 'ETH', side: 'buy', type: 'limit', amount: '1', price: '1670.13' }),
    ).rejects.toMatchObject({ code: 'invalid_request' });
    // integer prices are always allowed
    const ok = c([
      { body: META },
      { body: { status: 'ok', response: { type: 'order', data: { statuses: [{ resting: { oid: 3 } }] } } } },
    ]);
    await expect(ok.createOrder({ symbol: 'ETH', side: 'buy', type: 'limit', amount: '1', price: '123456' })).resolves.toMatchObject({ id: '3' });
  });

  it('maps exchange rejections onto the unified error taxonomy', async () => {
    const insufficient = new HyperliquidClient({
      venueId: 'hyperliquid',
      credentials: { privateKey: KEY },
      fetchImpl: mockFetch([
        { body: META },
        { body: { status: 'ok', response: { type: 'order', data: { statuses: [{ error: 'Insufficient margin to place order. asset=1' }] } } } },
      ]).f,
    });
    await expect(
      insufficient.createOrder({ symbol: 'ETH', side: 'buy', type: 'limit', amount: '1', price: '2000' }),
    ).rejects.toMatchObject({ name: 'VenueError', code: 'insufficient_funds' });

    const topLevel = new HyperliquidClient({
      venueId: 'hyperliquid',
      credentials: { privateKey: KEY },
      fetchImpl: mockFetch([{ body: META }, { body: { status: 'err', response: 'User or API Wallet does not exist.' } }]).f,
    });
    await expect(
      topLevel.createOrder({ symbol: 'ETH', side: 'buy', type: 'limit', amount: '1', price: '2000' }),
    ).rejects.toMatchObject({ name: 'VenueError', code: 'invalid_request' });

    const unknownCoin = new HyperliquidClient({ venueId: 'hyperliquid', fetchImpl: mockFetch([{ body: META }]).f });
    await expect(unknownCoin.fetchMarket('NOPE')).rejects.toMatchObject({ code: 'bad_symbol' });

    const geo = new HyperliquidClient({
      venueId: 'hyperliquid',
      fetchImpl: mockFetch([{ status: 451, body: 'blocked' }]).f,
    });
    await expect(geo.fetchTicker('ETH')).rejects.toMatchObject({ code: 'venue_unavailable', httpStatus: 451 });
  });

  it('cancels by numeric oid with the exact cancel action', async () => {
    const { f, calls } = mockFetch([
      { body: META },
      { body: { status: 'ok', response: { type: 'cancel', data: { statuses: ['success'] } } } },
    ]);
    await new HyperliquidClient({ venueId: 'hyperliquid', credentials: { privateKey: KEY }, fetchImpl: f }).cancelOrder(
      '77738308',
      'ETH',
    );
    expect(body(calls[1]!).action).toEqual({ type: 'cancel', cancels: [{ a: 1, o: 77738308 }] });
  });

  it('reads open orders / fills / balances / positions for accountAddress without any key (agent-mode reads)', async () => {
    const user = '0x5e9ee1089755c3435139848e47e6635505d5a13a';
    const { f, calls } = mockFetch([
      {
        body: [
          { coin: 'ETH', oid: 1, side: 'B', limitPx: '2000', sz: '0.3', origSz: '1.0', timestamp: 5 },
          { coin: 'BTC', oid: 2, side: 'A', limitPx: '60000', sz: '1', origSz: '1', timestamp: 6 },
        ],
      },
      { body: { marginSummary: { accountValue: '1250.5' }, withdrawable: '1000.0' } },
      {
        body: {
          assetPositions: [
            {
              position: {
                coin: 'ETH',
                szi: '-2.5',
                entryPx: '1900.0',
                unrealizedPnl: '12.5',
                liquidationPx: '2500.1',
                leverage: { type: 'cross', value: 5 },
              },
            },
          ],
        },
      },
    ]);
    const client = new HyperliquidClient({
      venueId: 'hyperliquid',
      credentials: { accountAddress: user },
      fetchImpl: f,
    });
    const open = await client.fetchOpenOrders('ETH');
    expect(body(calls[0]!)).toEqual({ type: 'openOrders', user });
    expect(open).toHaveLength(1);
    expect(open[0]).toMatchObject({ id: '1', side: 'buy', amount: '1.0', filled: '0.7', status: 'open' });

    const balances = await client.fetchBalances();
    expect(balances).toEqual([{ asset: 'USDC', free: '1000.0', total: '1250.5' }]);

    const positions = await client.fetchPositions();
    expect(positions[0]).toMatchObject({ symbol: 'ETH', side: 'short', size: '-2.5', leverage: 5 });
  });

  it('defaults reads to the signer address when only a key is configured', async () => {
    const { f, calls } = mockFetch([{ body: { marginSummary: { accountValue: '0' }, withdrawable: '0' } }]);
    await new HyperliquidClient({ venueId: 'hyperliquid', credentials: { privateKey: KEY }, fetchImpl: f }).fetchBalances();
    expect(body(calls[0]!)).toEqual({ type: 'clearinghouseState', user: privateKeyToAddress(KEY) });
  });

  it('fetches funding from metaAndAssetCtxs', async () => {
    const { f } = mockFetch([
      { body: [META, [{ funding: '0.0000125', markPx: '60001' }, { funding: '-0.0000031', markPx: '1999.5' }]] },
    ]);
    const fr = await new HyperliquidClient({ venueId: 'hyperliquid', fetchImpl: f }).fetchFundingRate('ETH');
    expect(fr).toMatchObject({ symbol: 'ETH', fundingRate: '-0.0000031', markPrice: '1999.5' });
  });

  it('updates leverage with the exact action', async () => {
    const { f, calls } = mockFetch([{ body: META }, { body: { status: 'ok', response: { type: 'default' } } }]);
    await new HyperliquidClient({ venueId: 'hyperliquid', credentials: { privateKey: KEY }, fetchImpl: f }).setLeverage(
      'ETH',
      5,
      { cross: false },
    );
    expect(body(calls[1]!).action).toEqual({ type: 'updateLeverage', asset: 1, isCross: false, leverage: 5 });
  });

  it('approveAgent posts a user-signed action (master key signs; agent gains trade-only powers)', async () => {
    const { f, calls } = mockFetch([{ body: { status: 'ok', response: { type: 'default' } } }]);
    const agent = '0x' + '11'.repeat(20);
    await new HyperliquidClient({
      venueId: 'hyperliquid',
      sandbox: true,
      credentials: { privateKey: KEY },
      fetchImpl: f,
      nowMs: () => 1700000000000,
    }).approveAgent({ agentAddress: agent, agentName: 'lit-policy' });
    const sent = body(calls[0]!);
    expect(sent.action).toEqual({
      type: 'approveAgent',
      hyperliquidChain: 'Testnet',
      signatureChainId: '0x66eee',
      agentAddress: agent,
      agentName: 'lit-policy',
      nonce: 1700000000000,
    });
    expect(sent.nonce).toBe(1700000000000);
    expect((sent.signature as { v: number }).v).toBeGreaterThanOrEqual(27);
  });

  it('requires a signer for trading and an account for reads', async () => {
    const readless = new HyperliquidClient({ venueId: 'hyperliquid', fetchImpl: mockFetch([]).f });
    await expect(readless.fetchBalances()).rejects.toMatchObject({ code: 'auth' });
    const keyless = new HyperliquidClient({ venueId: 'hyperliquid', fetchImpl: mockFetch([{ body: META }]).f });
    await expect(
      keyless.createOrder({ symbol: 'ETH', side: 'buy', type: 'limit', amount: '1', price: '2000' }),
    ).rejects.toMatchObject({ code: 'auth' });
  });
});
