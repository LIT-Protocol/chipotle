import { VenueError, type VenueErrorCode } from '../errors';
import { httpRequest } from '../http';
import { resolveFetch } from '../transports';
import { addDec } from '../decimal';
import { es256Jwt, randomHex } from '../signing';
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

/**
 * Coinbase Advanced Trade (CDP keys, ES256 JWT per request).
 * NOTE: Advanced Trade has no real sandbox; `sandbox: true` is rejected
 * honestly rather than pretending (plan D5 covers how Tier-1 validation works).
 */

const HOST = 'api.coinbase.com';

const STATUS_MAP: Record<string, OrderStatus> = {
  OPEN: 'open',
  PENDING: 'open',
  QUEUED: 'open',
  UNTRIGGERED: 'open',
  FILLED: 'filled',
  CANCELLED: 'canceled',
  CANCEL_QUEUED: 'canceled',
  EXPIRED: 'expired',
  FAILED: 'rejected',
};

type Query = Array<[string, string]>;

export class CoinbaseClient implements VenueClient {
  readonly venueId = 'coinbase' as const;
  private readonly doFetch: FetchLike;

  constructor(private readonly cfg: VenueConfig) {
    if (cfg.venueId !== 'coinbase') {
      throw new VenueError(String(cfg.venueId), 'invalid_request', 'CoinbaseClient venueId must be coinbase');
    }
    this.doFetch = resolveFetch(cfg);
    if (cfg.sandbox) {
      throw new VenueError(
        'coinbase',
        'invalid_request',
        'Coinbase Advanced Trade has no functional sandbox; run read-only against live or use coinbaseexchange-style sandbox flows per plan D5',
      );
    }
  }

  private now(): number {
    return this.cfg.nowMs ? this.cfg.nowMs() : Date.now();
  }

  private toVenueSymbol(symbol: string): string {
    const [base, quote] = symbol.split('/');
    if (!base || !quote) {
      throw new VenueError(this.venueId, 'bad_symbol', `expected "BASE/QUOTE", got "${symbol}"`);
    }
    return `${base}-${quote}`.toUpperCase();
  }

  private fromVenueSymbol(productId: string): string {
    return productId.replace('-', '/');
  }

  private async request(
    method: 'GET' | 'POST',
    path: string,
    opts: { query?: Query; body?: unknown; auth?: boolean } = {},
  ): Promise<unknown> {
    const headers: Record<string, string> = {};
    if (opts.auth) {
      const creds = this.cfg.credentials;
      if (!creds) throw new VenueError(this.venueId, 'auth', 'this call requires credentials');
      // uri claim is method + host + path, query string excluded, per CDP docs.
      headers.Authorization = `Bearer ${es256Jwt({
        keyName: creds.apiKey,
        privateKey: creds.secret,
        uri: `${method} ${HOST}${path}`,
        nowMs: this.now(),
      })}`;
    }
    let body: string | undefined;
    if (opts.body !== undefined) {
      headers['content-type'] = 'application/json';
      body = JSON.stringify(opts.body);
    }
    const qs = (opts.query ?? []).map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join('&');
    const res = await httpRequest(this.doFetch, {
      method,
      url: `https://${HOST}${path}${qs ? `?${qs}` : ''}`,
      headers,
      body,
    });
    let parsed: unknown;
    try {
      parsed = JSON.parse(res.text);
    } catch {
      parsed = res.text;
    }
    if (!res.ok) throw this.mapError(res.status, parsed);
    return parsed;
  }

  private mapError(status: number, body: unknown): VenueError {
    const obj = (typeof body === 'object' && body !== null ? body : {}) as {
      error?: string;
      message?: string;
      error_details?: string;
    };
    const msg = obj.message ?? obj.error_details ?? obj.error ?? JSON.stringify(body).slice(0, 300);
    let code: VenueErrorCode = 'unknown';
    if (status === 401 || status === 403) code = 'auth';
    else if (status === 429) code = 'rate_limited';
    else if (status === 404) code = 'bad_symbol';
    else if (status >= 500) code = 'venue_unavailable';
    else if (status >= 400) code = 'invalid_request';
    return new VenueError(this.venueId, code, msg, status, obj.error);
  }

  async fetchTicker(symbol: string): Promise<Ticker> {
    const id = this.toVenueSymbol(symbol);
    const body = (await this.request('GET', `/api/v3/brokerage/market/products/${id}`)) as {
      price?: string;
    };
    if (body.price === undefined) {
      throw new VenueError(this.venueId, 'bad_symbol', `no price for ${symbol}`);
    }
    return { symbol, last: Number(body.price), ts: this.now(), info: body };
  }

  async fetchMarket(symbol: string): Promise<Market> {
    const id = this.toVenueSymbol(symbol);
    const body = (await this.request('GET', `/api/v3/brokerage/market/products/${id}`)) as {
      base_currency_id?: string;
      quote_currency_id?: string;
      quote_increment?: string;
      base_increment?: string;
      base_min_size?: string;
      quote_min_size?: string;
    };
    const [base, quote] = symbol.toUpperCase().split('/');
    return {
      symbol,
      base: body.base_currency_id ?? base ?? '',
      quote: body.quote_currency_id ?? quote ?? '',
      priceIncrement: body.quote_increment ?? '0',
      amountIncrement: body.base_increment ?? '0',
      minAmount: body.base_min_size,
      minNotional: body.quote_min_size,
      info: body,
    };
  }

  async fetchBalances(): Promise<Balance[]> {
    const body = (await this.request('GET', '/api/v3/brokerage/accounts', {
      query: [['limit', '250']],
      auth: true,
    })) as {
      accounts?: Array<{
        currency: string;
        available_balance?: { value: string };
        hold?: { value: string };
      }>;
    };
    return (body.accounts ?? [])
      .map((a) => {
        const free = a.available_balance?.value ?? '0';
        const hold = a.hold?.value ?? '0';
        return { asset: a.currency, free, total: addDec(free, hold) };
      })
      .filter((b) => Number(b.free) !== 0 || Number(b.total) !== 0);
  }

  async createOrder(req: OrderRequest): Promise<Order> {
    const productId = this.toVenueSymbol(req.symbol);
    let orderConfiguration: Record<string, unknown>;
    if (req.type === 'limit') {
      if (!req.amount || !req.price) {
        throw new VenueError(this.venueId, 'invalid_request', 'limit orders require amount and price');
      }
      if (req.timeInForce && req.timeInForce !== 'GTC') {
        throw new VenueError(this.venueId, 'invalid_request', `timeInForce ${req.timeInForce} not supported yet (v0 is GTC only)`);
      }
      orderConfiguration = { limit_limit_gtc: { base_size: req.amount, limit_price: req.price } };
    } else if (req.side === 'buy') {
      if (!req.quoteAmount) {
        throw new VenueError(
          this.venueId,
          'invalid_request',
          'coinbase market BUY orders take quoteAmount (quote-asset size), not amount',
        );
      }
      orderConfiguration = { market_market_ioc: { quote_size: req.quoteAmount } };
    } else {
      if (!req.amount) {
        throw new VenueError(this.venueId, 'invalid_request', 'market sell orders require amount');
      }
      orderConfiguration = { market_market_ioc: { base_size: req.amount } };
    }
    const clientOrderId = req.clientOrderId ?? randomHex(16);
    const body = (await this.request('POST', '/api/v3/brokerage/orders', {
      auth: true,
      body: {
        client_order_id: clientOrderId,
        product_id: productId,
        side: req.side.toUpperCase(),
        order_configuration: orderConfiguration,
      },
    })) as {
      success?: boolean;
      success_response?: { order_id?: string };
      error_response?: { error?: string; message?: string; preview_failure_reason?: string };
    };
    if (!body.success || !body.success_response?.order_id) {
      const err = body.error_response ?? {};
      const reason = err.message ?? err.preview_failure_reason ?? err.error ?? 'order rejected';
      const code: VenueErrorCode = /INSUFFICIENT/i.test(`${err.error} ${err.preview_failure_reason}`)
        ? 'insufficient_funds'
        : 'invalid_request';
      throw new VenueError(this.venueId, code, reason, undefined, err.error);
    }
    return {
      id: body.success_response.order_id,
      clientOrderId,
      symbol: req.symbol,
      side: req.side,
      type: req.type,
      // Advanced Trade's create response carries no fill state; poll fetchOpenOrders/fills.
      status: req.type === 'limit' ? 'open' : 'unknown',
      price: req.price,
      amount: req.amount,
      info: body,
    };
  }

  async cancelOrder(id: string, _symbol: string): Promise<void> {
    const body = (await this.request('POST', '/api/v3/brokerage/orders/batch_cancel', {
      auth: true,
      body: { order_ids: [id] },
    })) as { results?: Array<{ success?: boolean; failure_reason?: string }> };
    const r = body.results?.[0];
    if (!r?.success) {
      throw new VenueError(this.venueId, 'invalid_request', r?.failure_reason ?? `cancel of ${id} failed`);
    }
  }

  async fetchOpenOrders(symbol: string): Promise<Order[]> {
    const body = (await this.request('GET', '/api/v3/brokerage/orders/historical/batch', {
      auth: true,
      query: [
        ['order_status', 'OPEN'],
        ['product_id', this.toVenueSymbol(symbol)],
      ],
    })) as { orders?: Array<Record<string, unknown>> };
    return (body.orders ?? []).map((o) => this.mapOrder(o));
  }

  private mapOrder(o: Record<string, unknown>): Order {
    const config = (o.order_configuration ?? {}) as Record<string, { base_size?: string; limit_price?: string }>;
    const configKey = Object.keys(config)[0];
    const cfg = configKey ? config[configKey] : undefined;
    return {
      id: String(o.order_id),
      clientOrderId: o.client_order_id ? String(o.client_order_id) : undefined,
      symbol: this.fromVenueSymbol(String(o.product_id ?? '')),
      side: String(o.side).toLowerCase() === 'sell' ? 'sell' : 'buy',
      type: configKey?.startsWith('market') ? 'market' : configKey?.startsWith('limit') ? 'limit' : String(configKey ?? ''),
      status: STATUS_MAP[String(o.status)] ?? 'unknown',
      price: cfg?.limit_price,
      amount: cfg?.base_size,
      filled: o.filled_size !== undefined ? String(o.filled_size) : undefined,
      ts: typeof o.created_time === 'string' ? Date.parse(o.created_time) : undefined,
      info: o,
    };
  }

  async fetchMyTrades(symbol: string, opts: { limit?: number } = {}): Promise<Fill[]> {
    const query: Query = [['product_id', this.toVenueSymbol(symbol)]];
    if (opts.limit) query.push(['limit', String(opts.limit)]);
    const body = (await this.request('GET', '/api/v3/brokerage/orders/historical/fills', {
      auth: true,
      query,
    })) as { fills?: Array<Record<string, unknown>> };
    return (body.fills ?? []).map((f) => ({
      orderId: String(f.order_id),
      symbol,
      side:
        String(f.side ?? '').toUpperCase() === 'SELL'
          ? 'sell'
          : String(f.side ?? '').toUpperCase() === 'BUY'
            ? 'buy'
            : undefined,
      price: String(f.price),
      amount: String(f.size),
      fee: f.commission !== undefined ? String(f.commission) : undefined,
      ts: typeof f.trade_time === 'string' ? Date.parse(f.trade_time) : undefined,
      info: f,
    }));
  }
}
