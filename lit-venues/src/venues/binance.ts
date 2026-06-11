import { VenueError, type VenueErrorCode } from '../errors';
import { httpRequest } from '../http';
import { resolveFetch } from '../transports';
import { addDec } from '../decimal';
import { ed25519SignBase64, hmacSha256Hex } from '../signing';
import type {
  Balance,
  FetchLike,
  Fill,
  Market,
  Order,
  OrderRequest,
  OrderStatus,
  Ticker,
  VenueClient,
  VenueConfig,
} from '../types';

const BASES = {
  binance: 'https://api.binance.com',
  binanceTestnet: 'https://testnet.binance.vision',
  binanceus: 'https://api.binance.us',
} as const;

const RECV_WINDOW = '5000';

type Params = Array<[string, string]>;

function encodeParams(params: Params): string {
  return params.map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join('&');
}

const STATUS_MAP: Record<string, OrderStatus> = {
  NEW: 'open',
  PARTIALLY_FILLED: 'open',
  PENDING_NEW: 'open',
  FILLED: 'filled',
  CANCELED: 'canceled',
  PENDING_CANCEL: 'canceled',
  REJECTED: 'rejected',
  EXPIRED: 'expired',
  EXPIRED_IN_MATCH: 'expired',
};

export class BinanceClient implements VenueClient {
  readonly venueId: 'binance' | 'binanceus';
  private readonly base: string;
  private readonly doFetch: FetchLike;

  constructor(private readonly cfg: VenueConfig) {
    if (cfg.venueId !== 'binance' && cfg.venueId !== 'binanceus') {
      throw new VenueError(String(cfg.venueId), 'invalid_request', 'BinanceClient venueId must be binance or binanceus');
    }
    this.venueId = cfg.venueId;
    if (cfg.venueId === 'binanceus' && cfg.sandbox) {
      throw new VenueError('binanceus', 'invalid_request', 'binance.us has no public testnet');
    }
    this.base = cfg.venueId === 'binanceus' ? BASES.binanceus : cfg.sandbox ? BASES.binanceTestnet : BASES.binance;
    this.doFetch = resolveFetch(cfg);
  }

  private now(): number {
    return this.cfg.nowMs ? this.cfg.nowMs() : Date.now();
  }

  private toVenueSymbol(symbol: string): string {
    const [base, quote] = symbol.split('/');
    if (!base || !quote) {
      throw new VenueError(this.venueId, 'bad_symbol', `expected "BASE/QUOTE", got "${symbol}"`);
    }
    return `${base}${quote}`.toUpperCase();
  }

  private parse(status: number, text: string): unknown {
    let body: unknown;
    try {
      body = JSON.parse(text);
    } catch {
      body = text;
    }
    if (status >= 200 && status < 300) return body;
    throw this.mapError(status, body);
  }

  private mapError(status: number, body: unknown): VenueError {
    const venueCode = typeof body === 'object' && body !== null ? (body as { code?: number }).code : undefined;
    const msg =
      typeof body === 'object' && body !== null
        ? ((body as { msg?: string }).msg ?? JSON.stringify(body).slice(0, 300))
        : String(body).slice(0, 300);
    let code: VenueErrorCode = 'unknown';
    if (status === 451) {
      code = 'venue_unavailable';
      return new VenueError(
        this.venueId,
        code,
        'geo-blocked (HTTP 451): this venue does not permit the current egress IP region — route via an egress proxy (plan D4)',
        status,
        venueCode,
      );
    }
    if (status === 429 || status === 418 || venueCode === -1003) code = 'rate_limited';
    else if (venueCode === -1121 || venueCode === -1100) code = 'bad_symbol';
    else if (venueCode === -2010 || venueCode === -2018 || venueCode === -2019) code = 'insufficient_funds';
    else if (status === 401 || status === 403 || venueCode === -2014 || venueCode === -2015 || venueCode === -1022) code = 'auth';
    else if (status >= 500) code = 'venue_unavailable';
    else if (status >= 400) code = 'invalid_request';
    return new VenueError(this.venueId, code, msg, status, venueCode);
  }

  private async public_(path: string, params: Params = []): Promise<unknown> {
    const qs = encodeParams(params);
    const res = await httpRequest(this.doFetch, {
      method: 'GET',
      url: `${this.base}${path}${qs ? `?${qs}` : ''}`,
    });
    return this.parse(res.status, res.text);
  }

  private async signed(method: 'GET' | 'POST' | 'DELETE', path: string, params: Params): Promise<unknown> {
    const creds = this.cfg.credentials;
    if (!creds?.apiKey || !creds.secret) {
      throw new VenueError(this.venueId, 'auth', 'this call requires credentials (apiKey + secret)');
    }
    const all: Params = [...params, ['recvWindow', RECV_WINDOW], ['timestamp', String(this.now())]];
    const payload = encodeParams(all);
    const signature =
      creds.keyType === 'ed25519' ? ed25519SignBase64(creds.secret, payload) : hmacSha256Hex(creds.secret, payload);
    const res = await httpRequest(this.doFetch, {
      method,
      url: `${this.base}${path}?${payload}&signature=${encodeURIComponent(signature)}`,
      headers: { 'X-MBX-APIKEY': creds.apiKey },
    });
    return this.parse(res.status, res.text);
  }

  private mapOrder(symbol: string, o: Record<string, unknown>): Order {
    return {
      id: String(o.orderId),
      clientOrderId: o.clientOrderId ? String(o.clientOrderId) : undefined,
      symbol,
      side: String(o.side).toLowerCase() === 'sell' ? 'sell' : 'buy',
      type: String(o.type ?? '').toLowerCase(),
      status: STATUS_MAP[String(o.status)] ?? 'unknown',
      price: o.price !== undefined ? String(o.price) : undefined,
      amount: o.origQty !== undefined ? String(o.origQty) : undefined,
      filled: o.executedQty !== undefined ? String(o.executedQty) : undefined,
      ts: typeof o.transactTime === 'number' ? o.transactTime : typeof o.time === 'number' ? o.time : undefined,
      info: o,
    };
  }

  async fetchTicker(symbol: string): Promise<Ticker> {
    const v = this.toVenueSymbol(symbol);
    const body = (await this.public_('/api/v3/ticker/price', [['symbol', v]])) as { price: string };
    return { symbol, last: Number(body.price), ts: this.now(), info: body };
  }

  async fetchMarket(symbol: string): Promise<Market> {
    const cached = this.cfg.markets?.[symbol];
    if (cached) return cached;
    const v = this.toVenueSymbol(symbol);
    const body = (await this.public_('/api/v3/exchangeInfo', [['symbol', v]])) as {
      symbols?: Array<{
        baseAsset: string;
        quoteAsset: string;
        filters?: Array<Record<string, string>>;
      }>;
    };
    const m = body.symbols?.[0];
    if (!m) throw new VenueError(this.venueId, 'bad_symbol', `unknown market ${symbol}`);
    const filter = (type: string) => m.filters?.find((f) => f.filterType === type);
    const price = filter('PRICE_FILTER');
    const lot = filter('LOT_SIZE');
    const notional = filter('NOTIONAL') ?? filter('MIN_NOTIONAL');
    return {
      symbol,
      base: m.baseAsset,
      quote: m.quoteAsset,
      priceIncrement: price?.tickSize ?? '0',
      amountIncrement: lot?.stepSize ?? '0',
      minAmount: lot?.minQty,
      minNotional: notional?.minNotional,
      info: m,
    };
  }

  async fetchBalances(): Promise<Balance[]> {
    const body = (await this.signed('GET', '/api/v3/account', [])) as {
      balances?: Array<{ asset: string; free: string; locked: string }>;
    };
    return (body.balances ?? [])
      .filter((b) => Number(b.free) !== 0 || Number(b.locked) !== 0)
      .map((b) => ({ asset: b.asset, free: b.free, total: addDec(b.free, b.locked) }));
  }

  async createOrder(req: OrderRequest): Promise<Order> {
    if (req.reduceOnly) {
      throw new VenueError(this.venueId, 'invalid_request', 'reduceOnly is a perp concept; binance spot rejects it');
    }
    const v = this.toVenueSymbol(req.symbol);
    const params: Params = [
      ['symbol', v],
      ['side', req.side.toUpperCase()],
      ['type', req.type.toUpperCase()],
    ];
    if (req.type === 'limit') {
      if (!req.amount || !req.price) {
        throw new VenueError(this.venueId, 'invalid_request', 'limit orders require amount and price');
      }
      params.push(['timeInForce', req.timeInForce ?? 'GTC']);
      params.push(['quantity', req.amount]);
      params.push(['price', req.price]);
    } else {
      if (req.amount) params.push(['quantity', req.amount]);
      else if (req.quoteAmount) params.push(['quoteOrderQty', req.quoteAmount]);
      else throw new VenueError(this.venueId, 'invalid_request', 'market orders require amount or quoteAmount');
    }
    if (req.clientOrderId) params.push(['newClientOrderId', req.clientOrderId]);
    const body = (await this.signed('POST', '/api/v3/order', params)) as Record<string, unknown>;
    return this.mapOrder(req.symbol, body);
  }

  async cancelOrder(id: string, symbol: string): Promise<void> {
    await this.signed('DELETE', '/api/v3/order', [
      ['symbol', this.toVenueSymbol(symbol)],
      ['orderId', id],
    ]);
  }

  async fetchOpenOrders(symbol: string): Promise<Order[]> {
    const body = (await this.signed('GET', '/api/v3/openOrders', [
      ['symbol', this.toVenueSymbol(symbol)],
    ])) as Array<Record<string, unknown>>;
    return body.map((o) => this.mapOrder(symbol, o));
  }

  async fetchMyTrades(symbol: string, opts: { limit?: number } = {}): Promise<Fill[]> {
    const params: Params = [['symbol', this.toVenueSymbol(symbol)]];
    if (opts.limit) params.push(['limit', String(opts.limit)]);
    const body = (await this.signed('GET', '/api/v3/myTrades', params)) as Array<Record<string, unknown>>;
    return body.map((t) => ({
      orderId: String(t.orderId),
      symbol,
      side: t.isBuyer === true ? 'buy' : 'sell',
      price: String(t.price),
      amount: String(t.qty),
      fee: t.commission !== undefined ? String(t.commission) : undefined,
      feeAsset: t.commissionAsset !== undefined ? String(t.commissionAsset) : undefined,
      ts: typeof t.time === 'number' ? t.time : undefined,
      info: t,
    }));
  }
}
