import { afterEach, describe, expect, it } from 'vitest';
import { resolveFetch, litActionProxiedFetch } from '../src/transports';
import { BinanceClient } from '../src/venues/binance';

const g = globalThis as Record<string, unknown>;

afterEach(() => {
  delete g.Lit;
  delete g.LitActions;
});

describe('resolveFetch', () => {
  it('prefers an explicit fetchImpl over everything', () => {
    const fetchImpl = async () => ({ status: 200, ok: true, text: async () => '' });
    expect(resolveFetch({ fetchImpl, proxy: 'http://p' })).toBe(fetchImpl);
  });

  it('uses the in-TEE proxied-fetch op when a proxy is set', async () => {
    const seen: Array<Record<string, unknown>> = [];
    g.Lit = {
      Actions: {
        proxiedFetch: async (req: Record<string, unknown>) => {
          seen.push(req);
          return { status: 200, ok: true, text: async () => '{"price":"1"}' };
        },
      },
    };
    const f = resolveFetch({ proxy: 'http://user:pass@1.2.3.4:8080' });
    await f('https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT', { method: 'GET' });
    expect(seen[0]).toMatchObject({
      url: 'https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT',
      method: 'GET',
      proxy: 'http://user:pass@1.2.3.4:8080',
    });
  });

  it('throws when a proxy is requested but no runtime op and no fetchImpl exist', () => {
    expect(() => litActionProxiedFetch('http://p')).toThrow(/proxiedFetch is unavailable/);
  });

  it('falls back to global fetch when no fetchImpl and no proxy', () => {
    const fake = async () => ({ status: 200, ok: true, text: async () => '' });
    g.fetch = fake;
    try {
      expect(resolveFetch({})).toBe(fake);
    } finally {
      delete g.fetch;
    }
  });
});

describe('BinanceClient with a proxy + in-TEE op', () => {
  it('routes venue requests through Lit.Actions.proxiedFetch carrying the proxy URL', async () => {
    const calls: Array<Record<string, unknown>> = [];
    g.LitActions = {
      proxiedFetch: async (req: Record<string, unknown>) => {
        calls.push(req);
        return { status: 200, ok: true, text: async () => '{"price":"63000.00"}' };
      },
    };
    const binance = new BinanceClient({ venueId: 'binance', proxy: 'http://u:p@5.6.7.8:9999' });
    const ticker = await binance.fetchTicker('BTC/USDT');
    expect(ticker.last).toBe(63000);
    expect(calls[0]).toMatchObject({
      url: 'https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT',
      proxy: 'http://u:p@5.6.7.8:9999',
    });
  });
});
