export type VenueId = 'binance' | 'binanceus' | 'coinbase';

export type KeyType = 'hmac' | 'ed25519' | 'es256-jwt';

export interface VenueCredentials {
  apiKey: string;
  /**
   * Meaning depends on keyType:
   *  - hmac: the API secret string (Binance)
   *  - ed25519: PKCS8 PEM, raw hex (64 chars), or base64 private key (Binance Ed25519 keys)
   *  - es256-jwt: EC P-256 private key PEM, SEC1 or PKCS8 (Coinbase CDP keys)
   */
  secret: string;
  keyType?: KeyType;
}

/**
 * Minimal structural fetch type so the library typechecks against the Lit
 * Actions runtime's wrapped fetch, browser fetch, and Node fetch alike
 * without pulling in DOM lib types.
 */
export type FetchLike = (
  url: string,
  init?: Record<string, unknown>,
) => Promise<{ status: number; ok: boolean; text(): Promise<string> }>;

export interface VenueConfig {
  venueId: VenueId;
  credentials?: VenueCredentials;
  /** Route to the venue's public testnet where one exists (binance → testnet.binance.vision). */
  sandbox?: boolean;
  /**
   * Egress proxy URL (http(s)://user:pass@host:port). Forwarded to the runtime
   * fetch as `litProxy` (chipotle plan D4/M2); inert on runtimes without it.
   */
  proxy?: string;
  /** Override fetch. Defaults to globalThis.fetch. */
  fetchImpl?: FetchLike;
  /** Override clock (tests / deterministic signatures). Milliseconds since epoch. */
  nowMs?: () => number;
}

/** Amounts and prices are decimal strings end to end — no float drift in finance code. */
export interface Ticker {
  symbol: string;
  last: number;
  ts: number;
  info?: unknown;
}

export interface Market {
  symbol: string;
  base: string;
  quote: string;
  /** Price must be a multiple of this (exchange tick size). */
  priceIncrement: string;
  /** Base amount must be a multiple of this (lot/step size). */
  amountIncrement: string;
  minAmount?: string;
  minNotional?: string;
  info?: unknown;
}

export interface Balance {
  asset: string;
  free: string;
  total: string;
}

export interface OrderRequest {
  symbol: string;
  side: 'buy' | 'sell';
  type: 'limit' | 'market';
  /** Base-asset amount. Required except for coinbase market BUY, which takes quoteAmount. */
  amount?: string;
  /** Quote-asset amount — coinbase market BUY orders only. */
  quoteAmount?: string;
  price?: string;
  clientOrderId?: string;
  timeInForce?: 'GTC' | 'IOC' | 'FOK';
}

export type OrderStatus = 'open' | 'filled' | 'canceled' | 'rejected' | 'expired' | 'unknown';

export interface Order {
  id: string;
  clientOrderId?: string;
  symbol: string;
  side: 'buy' | 'sell';
  type: string;
  status: OrderStatus;
  price?: string;
  amount?: string;
  filled?: string;
  ts?: number;
  info?: unknown;
}

export interface Fill {
  orderId: string;
  symbol: string;
  side?: 'buy' | 'sell';
  price: string;
  amount: string;
  fee?: string;
  feeAsset?: string;
  ts?: number;
  info?: unknown;
}

export interface VenueClient {
  readonly venueId: VenueId;
  fetchTicker(symbol: string): Promise<Ticker>;
  fetchMarket(symbol: string): Promise<Market>;
  fetchBalances(): Promise<Balance[]>;
  createOrder(req: OrderRequest): Promise<Order>;
  cancelOrder(id: string, symbol: string): Promise<void>;
  /** Symbol is required by design: keeps responses small and unambiguous within action quotas. */
  fetchOpenOrders(symbol: string): Promise<Order[]>;
  fetchMyTrades(symbol: string, opts?: { limit?: number }): Promise<Fill[]>;
}
