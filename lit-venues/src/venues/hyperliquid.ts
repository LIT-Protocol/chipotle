import { VenueError, type VenueErrorCode } from '../errors';
import { httpRequest } from '../http';
import { resolveFetch } from '../transports';
import { addDec, applyBps, decimalsOf, floorToSigFigs, sigFigsOf, subDec, wireDecimal } from '../decimal';
import { privateKeyToAddress, rawKeySigner, type SignFn } from '../eip712';
import {
  APPROVE_AGENT_FIELDS,
  HYPERLIQUID_SIGNATURE_CHAIN_ID,
  signL1Action,
  signUserSignedAction,
} from './hyperliquid-signing';
import type {
  Balance,
  FetchLike,
  Fill,
  FundingRate,
  Market,
  Order,
  OrderRequest,
  Position,
  Ticker,
  VenueClient,
  VenueConfig,
} from '../types';

/**
 * Hyperliquid perps — the first PKP-native venue (plan D8). No API key
 * exists: every trading action is an EIP-712 signature from a secp256k1 key,
 * which in a Lit Action is the action-bound TEE key, connected venue-side as
 * an approved agent wallet (default; structurally cannot withdraw) or as the
 * master account.
 *
 * v1 scope is perps (spot/vaults/HIP-3 are plan non-goals). REST only:
 * POST /info (public) + POST /exchange (signed).
 */

const BASES = {
  mainnet: 'https://api.hyperliquid.xyz',
  testnet: 'https://api.hyperliquid-testnet.xyz',
} as const;

/** Perp price rule: decimals ≤ MAX_DECIMALS − szDecimals (spot would be 8). */
const MAX_DECIMALS = 6;
const MAX_SIG_FIGS = 5;
/** Venue-wide minimum order notional in USDC. */
const MIN_NOTIONAL = '10';

interface HlAssetMeta {
  name: string;
  szDecimals: number;
  maxLeverage?: number;
  onlyIsolated?: boolean;
}

interface HlMeta {
  universe: HlAssetMeta[];
}

interface ExchangeOrderStatus {
  resting?: { oid: number };
  filled?: { oid: number; totalSz?: string; avgPx?: string };
  error?: string;
}

function pow10Neg(n: number): string {
  return n <= 0 ? '1' : `0.${'0'.repeat(n - 1)}1`;
}

export class HyperliquidClient implements VenueClient {
  readonly venueId = 'hyperliquid' as const;
  private readonly base: string;
  private readonly doFetch: FetchLike;
  private metaCache?: HlMeta;
  private lastNonce = 0;

  constructor(private readonly cfg: VenueConfig) {
    if (cfg.venueId !== 'hyperliquid') {
      throw new VenueError(String(cfg.venueId), 'invalid_request', 'HyperliquidClient venueId must be hyperliquid');
    }
    this.base = cfg.sandbox ? BASES.testnet : BASES.mainnet;
    this.doFetch = resolveFetch(cfg);
  }

  private now(): number {
    return this.cfg.nowMs ? this.cfg.nowMs() : Date.now();
  }

  /**
   * Per-signer monotonic nonce. Hyperliquid keys actions by (signer, nonce)
   * and rejects stale/duplicate nonces, so two actions signed in the same
   * millisecond — or a clock that doesn't advance — would collide. Stepping
   * past the last-used value guarantees strictly increasing nonces from one
   * client instance (the one-agent-per-connection model, plan D8).
   */
  private nextNonce(): number {
    const n = Math.max(this.now(), this.lastNonce + 1);
    this.lastNonce = n;
    return n;
  }

  private get isMainnet(): boolean {
    return !this.cfg.sandbox;
  }

  /** "ETH", "ETH/USDC" and ccxt-style "ETH/USDC:USDC" all name the ETH perp. Case is preserved — Hyperliquid coins like "kPEPE" are case-sensitive. */
  private coin(symbol: string): string {
    const c = symbol.split('/')[0]?.trim();
    if (!c) throw new VenueError(this.venueId, 'bad_symbol', `expected a coin or "COIN/USDC:USDC", got "${symbol}"`);
    return c;
  }

  private accountCache?: string;

  /**
   * Address whose state is read. Resolution order:
   *   1. explicit credentials.accountAddress,
   *   2. if the signing key is a registered AGENT (API wallet), its master —
   *      resolved once via the venue's userRole endpoint and cached. Agent
   *      wallets are the default connection mode (plan D8) and their own
   *      account is always empty; reading it instead of the master is the
   *      footgun this lookup removes,
   *   3. the signer's own address (master mode).
   */
  private async account(): Promise<string> {
    if (this.accountCache) return this.accountCache;
    const creds = this.cfg.credentials;
    if (creds?.accountAddress) {
      this.accountCache = creds.accountAddress.toLowerCase();
      return this.accountCache;
    }
    if (!creds?.privateKey) {
      throw new VenueError(
        this.venueId,
        'auth',
        'reads require credentials.accountAddress (agent mode) or credentials.privateKey',
      );
    }
    const self = privateKeyToAddress(creds.privateKey);
    const role = (await this.info({ type: 'userRole', user: self })) as {
      role?: string;
      data?: { user?: string };
    };
    this.accountCache = role?.role === 'agent' && role.data?.user ? role.data.user.toLowerCase() : self;
    return this.accountCache;
  }

  private signer(): SignFn {
    if (this.cfg.signFn) return this.cfg.signFn;
    const pk = this.cfg.credentials?.privateKey;
    if (!pk) {
      throw new VenueError(
        this.venueId,
        'auth',
        'trading requires credentials.privateKey (the PKP/agent key) or a custom signFn',
      );
    }
    return rawKeySigner(pk);
  }

  private httpError(status: number, text: string): VenueError {
    let code: VenueErrorCode = 'unknown';
    if (status === 429) code = 'rate_limited';
    else if (status === 451 || status === 403) {
      return new VenueError(
        this.venueId,
        'venue_unavailable',
        `geo-blocked or forbidden (HTTP ${status}): hyperliquid restricts some egress regions — route via an egress proxy (plan D4) if the CVM region is blocked`,
        status,
      );
    } else if (status >= 500) code = 'venue_unavailable';
    else if (status >= 400) code = 'invalid_request';
    return new VenueError(this.venueId, code, text.slice(0, 300), status);
  }

  /** Map /exchange per-order error strings onto the unified taxonomy. */
  private exchangeError(message: string): VenueError {
    let code: VenueErrorCode = 'invalid_request';
    if (/insufficient/i.test(message)) code = 'insufficient_funds';
    else if (/rate limit|too many/i.test(message)) code = 'rate_limited';
    else if (/asset|coin/i.test(message) && /invalid|unknown|not found/i.test(message)) code = 'bad_symbol';
    return new VenueError(this.venueId, code, message);
  }

  private async post(path: '/info' | '/exchange', body: unknown): Promise<unknown> {
    const res = await httpRequest(this.doFetch, {
      method: 'POST',
      url: `${this.base}${path}`,
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) throw this.httpError(res.status, res.text);
    try {
      return JSON.parse(res.text);
    } catch {
      throw new VenueError(this.venueId, 'venue_unavailable', `non-JSON response: ${res.text.slice(0, 120)}`);
    }
  }

  private async info(body: Record<string, unknown>): Promise<unknown> {
    return this.post('/info', body);
  }

  private async meta(): Promise<HlMeta> {
    if (!this.metaCache) {
      this.metaCache = (await this.info({ type: 'meta' })) as HlMeta;
    }
    return this.metaCache;
  }

  private async asset(coin: string): Promise<{ id: number; meta: HlAssetMeta }> {
    const { universe } = await this.meta();
    const id = universe.findIndex((a) => a.name === coin);
    if (id < 0) throw new VenueError(this.venueId, 'bad_symbol', `unknown coin "${coin}" (perps universe)`);
    return { id, meta: universe[id]! };
  }

  /** Sign and submit an L1 (trading) action. One signature covers the whole batched action. */
  private async exchange(action: Record<string, unknown>): Promise<unknown> {
    const nonce = this.nextNonce();
    const vaultAddress = this.cfg.credentials?.vaultAddress?.toLowerCase();
    const signature = await signL1Action(this.signer(), action, nonce, {
      isMainnet: this.isMainnet,
      vaultAddress,
    });
    const body: Record<string, unknown> = { action, nonce, signature };
    if (vaultAddress) body.vaultAddress = vaultAddress;
    const resp = (await this.post('/exchange', body)) as {
      status?: string;
      response?: { type?: string; data?: { statuses?: ExchangeOrderStatus[] | string[] } } | string;
    };
    if (resp.status !== 'ok') {
      const msg = typeof resp.response === 'string' ? resp.response : JSON.stringify(resp).slice(0, 300);
      throw this.exchangeError(msg);
    }
    return resp;
  }

  private validatePx(px: string, szDecimals: number): void {
    if (!px.includes('.')) return; // integer prices are always valid on hyperliquid
    const maxDp = MAX_DECIMALS - szDecimals;
    if (decimalsOf(px) > maxDp) {
      throw new VenueError(
        this.venueId,
        'invalid_request',
        `price ${px} has more than ${maxDp} decimals (MAX_DECIMALS − szDecimals rule)`,
      );
    }
    if (sigFigsOf(px) > MAX_SIG_FIGS) {
      throw new VenueError(this.venueId, 'invalid_request', `price ${px} exceeds ${MAX_SIG_FIGS} significant figures`);
    }
  }

  async fetchTicker(symbol: string): Promise<Ticker> {
    const coin = this.coin(symbol);
    const mids = (await this.info({ type: 'allMids' })) as Record<string, string>;
    const mid = mids[coin];
    if (mid === undefined) throw new VenueError(this.venueId, 'bad_symbol', `no mid for coin "${coin}"`);
    return { symbol, last: Number(mid), ts: this.now(), info: { mid } };
  }

  async fetchMarket(symbol: string): Promise<Market> {
    const cached = this.cfg.markets?.[symbol];
    if (cached) return cached;
    const coin = this.coin(symbol);
    const { id, meta } = await this.asset(coin);
    return {
      symbol,
      base: coin,
      quote: 'USDC',
      priceIncrement: pow10Neg(MAX_DECIMALS - meta.szDecimals),
      amountIncrement: pow10Neg(meta.szDecimals),
      minAmount: pow10Neg(meta.szDecimals),
      minNotional: MIN_NOTIONAL,
      info: { assetId: id, szDecimals: meta.szDecimals, maxLeverage: meta.maxLeverage },
    };
  }

  /**
   * Unified accounts (the venue's newer default) margin perps directly from
   * the spot-side balance and disable manual spot↔perp transfers — a
   * perp-only read shows ~0 while the account trades happily (found live,
   * plan D8). So this merges both pools: one USDC row = perp equity + spot
   * USDC, with `free` preferring the venue's own available-after-maintenance
   * figure; other spot coins follow as extra rows. Costs 2 /info calls.
   */
  async fetchBalances(): Promise<Balance[]> {
    const user = await this.account();
    const perp = (await this.info({ type: 'clearinghouseState', user })) as {
      marginSummary?: { accountValue?: string };
      withdrawable?: string;
    };
    const spot = (await this.info({ type: 'spotClearinghouseState', user })) as {
      balances?: Array<{ coin: string; token?: number; total: string; hold?: string }>;
      tokenToAvailableAfterMaintenance?: Array<[number, string]>;
    };
    const availByToken = new Map(spot.tokenToAvailableAfterMaintenance ?? []);
    let usdcFree = perp.withdrawable ?? '0';
    let usdcTotal = perp.marginSummary?.accountValue ?? '0';
    const rest: Balance[] = [];
    for (const b of spot.balances ?? []) {
      const free = availByToken.get(b.token ?? -1) ?? subDec(b.total, b.hold ?? '0');
      if (b.coin === 'USDC') {
        usdcFree = addDec(usdcFree, free);
        usdcTotal = addDec(usdcTotal, b.total);
      } else if (Number(b.total) !== 0) {
        rest.push({ asset: b.coin, free, total: b.total });
      }
    }
    return [{ asset: 'USDC', free: usdcFree, total: usdcTotal }, ...rest];
  }

  async createOrder(req: OrderRequest): Promise<Order> {
    const coin = this.coin(req.symbol);
    const { id: assetId, meta } = await this.asset(coin);
    if (!req.amount) throw new VenueError(this.venueId, 'invalid_request', 'orders require amount (base size)');
    if (req.quoteAmount) {
      throw new VenueError(this.venueId, 'invalid_request', 'hyperliquid sizes orders in base units; quoteAmount is unsupported');
    }
    const sz = wireDecimal(req.amount);
    if (decimalsOf(sz) > meta.szDecimals) {
      throw new VenueError(
        this.venueId,
        'invalid_request',
        `amount ${sz} exceeds szDecimals=${meta.szDecimals} for ${coin}`,
      );
    }

    let px: string;
    let tif: 'Gtc' | 'Ioc' | 'Alo';
    if (req.type === 'limit') {
      if (!req.price) throw new VenueError(this.venueId, 'invalid_request', 'limit orders require price');
      px = wireDecimal(req.price);
      const tifMap = { GTC: 'Gtc', IOC: 'Ioc' } as const;
      const wanted = req.timeInForce ?? 'GTC';
      if (!(wanted in tifMap)) {
        throw new VenueError(this.venueId, 'invalid_request', `timeInForce ${wanted} unsupported on hyperliquid (use GTC or IOC)`);
      }
      tif = tifMap[wanted as keyof typeof tifMap];
    } else {
      // Market = aggressive IOC limit (the venue has no native market order).
      tif = 'Ioc';
      if (req.price) {
        px = wireDecimal(req.price);
      } else {
        const mids = (await this.info({ type: 'allMids' })) as Record<string, string>;
        const mid = mids[coin];
        if (mid === undefined) throw new VenueError(this.venueId, 'bad_symbol', `no mid for coin "${coin}"`);
        const bps = this.cfg.slippageBps ?? 500;
        const aggressive = applyBps(wireDecimal(mid), req.side === 'buy' ? bps : -bps, MAX_DECIMALS - meta.szDecimals);
        px = floorToSigFigs(aggressive, MAX_SIG_FIGS);
      }
    }
    this.validatePx(px, meta.szDecimals);

    let cloid: string | undefined;
    if (req.clientOrderId) {
      if (!/^0x[0-9a-fA-F]{32}$/.test(req.clientOrderId)) {
        throw new VenueError(
          this.venueId,
          'invalid_request',
          'hyperliquid clientOrderId (cloid) must be 128-bit hex: 0x + 32 hex chars',
        );
      }
      cloid = req.clientOrderId.toLowerCase();
    }

    // Field order below is load-bearing: the msgpack action hash commits to it.
    const wire: Record<string, unknown> = {
      a: assetId,
      b: req.side === 'buy',
      p: px,
      s: sz,
      r: req.reduceOnly ?? false,
      t: { limit: { tif } },
    };
    if (cloid) wire.c = cloid;
    const action: Record<string, unknown> = { type: 'order', orders: [wire], grouping: 'na' };
    if (this.cfg.builder) {
      action.builder = { b: this.cfg.builder.address.toLowerCase(), f: this.cfg.builder.feeTenthBps };
    }

    const resp = (await this.exchange(action)) as {
      response?: { data?: { statuses?: ExchangeOrderStatus[] } };
    };
    const status = resp.response?.data?.statuses?.[0];
    if (!status) throw new VenueError(this.venueId, 'unknown', 'no order status in exchange response');
    if (status.error) throw this.exchangeError(status.error);
    const filled = status.filled;
    const resting = status.resting;
    return {
      id: String(filled?.oid ?? resting?.oid ?? ''),
      clientOrderId: cloid,
      symbol: req.symbol,
      side: req.side,
      type: req.type,
      status: filled ? 'filled' : 'open',
      price: filled?.avgPx ?? px,
      amount: sz,
      filled: filled?.totalSz ?? '0',
      ts: this.now(),
      info: status,
    };
  }

  async cancelOrder(id: string, symbol: string): Promise<void> {
    const coin = this.coin(symbol);
    const { id: assetId } = await this.asset(coin);
    const oid = Number(id);
    if (!Number.isInteger(oid)) throw new VenueError(this.venueId, 'invalid_request', `order id must be numeric, got "${id}"`);
    const action = { type: 'cancel', cancels: [{ a: assetId, o: oid }] };
    const resp = (await this.exchange(action)) as {
      response?: { data?: { statuses?: Array<string | { error?: string }> } };
    };
    const status = resp.response?.data?.statuses?.[0];
    if (typeof status === 'object' && status?.error) throw this.exchangeError(status.error);
  }

  async fetchOpenOrders(symbol: string): Promise<Order[]> {
    const coin = this.coin(symbol);
    const orders = (await this.info({ type: 'openOrders', user: await this.account() })) as Array<{
      coin: string;
      oid: number;
      side: 'B' | 'A';
      limitPx: string;
      sz: string;
      origSz: string;
      timestamp: number;
      cloid?: string;
    }>;
    return orders
      .filter((o) => o.coin === coin)
      .map((o) => ({
        id: String(o.oid),
        clientOrderId: o.cloid,
        symbol,
        side: o.side === 'A' ? ('sell' as const) : ('buy' as const),
        type: 'limit',
        status: 'open' as const,
        price: o.limitPx,
        amount: o.origSz,
        filled: subDec(o.origSz, o.sz),
        ts: o.timestamp,
        info: o,
      }));
  }

  async fetchMyTrades(symbol: string, opts: { limit?: number } = {}): Promise<Fill[]> {
    const coin = this.coin(symbol);
    const fills = (await this.info({ type: 'userFills', user: await this.account() })) as Array<{
      coin: string;
      px: string;
      sz: string;
      side: 'B' | 'A';
      time: number;
      oid: number;
      fee?: string;
      feeToken?: string;
    }>;
    const mine = fills.filter((f) => f.coin === coin);
    const limited = opts.limit ? mine.slice(0, opts.limit) : mine;
    return limited.map((f) => ({
      orderId: String(f.oid),
      symbol,
      side: f.side === 'A' ? ('sell' as const) : ('buy' as const),
      price: f.px,
      amount: f.sz,
      fee: f.fee,
      feeAsset: f.feeToken,
      ts: f.time,
      info: f,
    }));
  }

  // ---- perp surface (plan D8) -------------------------------------------

  async fetchPositions(): Promise<Position[]> {
    const state = (await this.info({ type: 'clearinghouseState', user: await this.account() })) as {
      assetPositions?: Array<{
        position: {
          coin: string;
          szi: string;
          entryPx?: string;
          unrealizedPnl?: string;
          liquidationPx?: string | null;
          leverage?: { type: string; value: number };
        };
      }>;
    };
    return (state.assetPositions ?? [])
      .filter((p) => Number(p.position.szi) !== 0)
      .map((p) => ({
        symbol: p.position.coin,
        side: p.position.szi.startsWith('-') ? ('short' as const) : ('long' as const),
        size: p.position.szi,
        entryPrice: p.position.entryPx,
        unrealizedPnl: p.position.unrealizedPnl,
        leverage: p.position.leverage?.value,
        liquidationPrice: p.position.liquidationPx ?? undefined,
        info: p.position,
      }));
  }

  async setLeverage(symbol: string, leverage: number, opts: { cross?: boolean } = {}): Promise<void> {
    if (!Number.isInteger(leverage) || leverage < 1) {
      throw new VenueError(this.venueId, 'invalid_request', 'leverage must be a positive integer');
    }
    const { id: assetId } = await this.asset(this.coin(symbol));
    // Field order is load-bearing (msgpack action hash).
    const action = { type: 'updateLeverage', asset: assetId, isCross: opts.cross ?? true, leverage };
    await this.exchange(action);
  }

  async fetchFundingRate(symbol: string): Promise<FundingRate> {
    const coin = this.coin(symbol);
    const [meta, ctxs] = (await this.info({ type: 'metaAndAssetCtxs' })) as [
      HlMeta,
      Array<{ funding: string; markPx?: string }>,
    ];
    const idx = meta.universe.findIndex((a) => a.name === coin);
    const ctx = idx >= 0 ? ctxs[idx] : undefined;
    if (!ctx) throw new VenueError(this.venueId, 'bad_symbol', `unknown coin "${coin}"`);
    return { symbol, fundingRate: ctx.funding, markPrice: ctx.markPx, ts: this.now(), info: ctx };
  }

  // ---- connect-time helper (plan D8 PKP-as-agent mode) -------------------

  /**
   * Approve `agentAddress` (the PKP's eth address) as a trading agent for the
   * signing master account. User-signed action — must be signed by the MASTER
   * key, typically once at connect time. The agent it authorizes can sign
   * orders/cancels but never withdrawals or transfers.
   */
  async approveAgent(req: { agentAddress: string; agentName: string }): Promise<void> {
    if (!/^0x[0-9a-fA-F]{40}$/.test(req.agentAddress)) {
      throw new VenueError(this.venueId, 'invalid_request', 'agentAddress must be a 0x address');
    }
    if (!req.agentName) {
      throw new VenueError(this.venueId, 'invalid_request', 'agentName is required (named agents are auditable and individually revocable)');
    }
    const nonce = this.nextNonce();
    const hyperliquidChain = this.isMainnet ? 'Mainnet' : 'Testnet';
    const message = {
      hyperliquidChain,
      agentAddress: req.agentAddress.toLowerCase(),
      agentName: req.agentName,
      nonce,
    };
    const signature = await signUserSignedAction(
      this.signer(),
      message,
      APPROVE_AGENT_FIELDS,
      'HyperliquidTransaction:ApproveAgent',
    );
    const action = {
      type: 'approveAgent',
      hyperliquidChain,
      signatureChainId: HYPERLIQUID_SIGNATURE_CHAIN_ID,
      agentAddress: req.agentAddress.toLowerCase(),
      agentName: req.agentName,
      nonce,
    };
    const resp = (await this.post('/exchange', { action, nonce, signature })) as { status?: string; response?: unknown };
    if (resp.status !== 'ok') {
      const msg = typeof resp.response === 'string' ? resp.response : JSON.stringify(resp).slice(0, 300);
      throw this.exchangeError(msg);
    }
  }
}
