import type { SignFn } from './eip712';

export type VenueId = 'binance' | 'binanceus' | 'coinbase' | 'hyperliquid';

export type KeyType = 'hmac' | 'ed25519' | 'es256-jwt' | 'pkp-eip712';

export interface VenueCredentials {
  /** API key id. Required for API-key venues (binance, coinbase); absent for PKP-native venues. */
  apiKey?: string;
  /**
   * Meaning depends on keyType:
   *  - hmac: the API secret string (Binance)
   *  - ed25519: PKCS8 PEM, raw hex (64 chars), or base64 private key (Binance Ed25519 keys)
   *  - es256-jwt: EC P-256 private key PEM, SEC1 or PKCS8 (Coinbase CDP keys)
   */
  secret?: string;
  keyType?: KeyType;
  /**
   * pkp-eip712 venues (hyperliquid): hex secp256k1 private key of the signing
   * wallet — in a Lit Action this is the action-bound TEE key
   * (`Lit.Actions.getLitActionPrivateKey()`), connected venue-side either as
   * an approved agent wallet (default; cannot withdraw) or as the master.
   */
  privateKey?: string;
  /**
   * pkp-eip712: the account whose balances/orders are read. Defaults to the
   * signer's own address (master mode); in agent mode set it to the master
   * address the agent was approved for. Reads work with this alone (no key).
   */
  accountAddress?: string;
  /** pkp-eip712: trade on behalf of a vault/subaccount — signed into the action hash. */
  vaultAddress?: string;
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
  /** Route to the venue's public testnet where one exists (binance → testnet.binance.vision, hyperliquid → api.hyperliquid-testnet.xyz). */
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
  /**
   * Pre-fetched market metadata keyed by unified symbol. When present,
   * fetchMarket() answers from here without an HTTP round-trip — lets actions
   * inject rules gathered out-of-band and stay inside the 50-fetch quota
   * (plan M1 markets-cache injection).
   */
  markets?: Record<string, Market>;
  /** pkp-eip712: custom EIP-712 digest signer (MPC hook). Defaults to signing with credentials.privateKey. */
  signFn?: SignFn;
  /** hyperliquid market orders are aggressive IOC limits; this caps the price slippage applied to mid. Default 500 (5%). */
  slippageBps?: number;
  /** hyperliquid builder code: collect a per-order fee (user must have signed approveBuilderFee for this address). f is in tenths of a basis point. */
  builder?: { address: string; feeTenthBps: number };
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
  /** Price must be a multiple of this (exchange tick size). For hyperliquid this is the MAX_DECIMALS−szDecimals bound; the 5-sig-fig rule is enforced at order time. */
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
  /** Perp venues only (hyperliquid): order may only reduce an existing position. Spot venues reject it. */
  reduceOnly?: boolean;
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

/** Open perp position (D8 perp surface). */
export interface Position {
  symbol: string;
  side: 'long' | 'short';
  /** Signed base size as reported by the venue (negative = short). */
  size: string;
  entryPrice?: string;
  unrealizedPnl?: string;
  leverage?: number;
  liquidationPrice?: string;
  info?: unknown;
}

/** Funding snapshot for a perp market (D8 perp surface). */
export interface FundingRate {
  symbol: string;
  /** Current funding rate as a decimal string, per the venue's funding interval. */
  fundingRate: string;
  markPrice?: string;
  ts: number;
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
  // ---- optional perp surface (plan D8) — present on perp venues, gated per-venue by the conformance suite
  fetchPositions?(): Promise<Position[]>;
  setLeverage?(symbol: string, leverage: number, opts?: { cross?: boolean }): Promise<void>;
  fetchFundingRate?(symbol: string): Promise<FundingRate>;
}
