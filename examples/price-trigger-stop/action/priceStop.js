// Lit Action: a stop-loss that can only ever act inside its policy fences.
//
// A price poller (a lit-triggers schedule trigger, or scripts/poll.js) merely
// INVOKES this action on a heartbeat. The action itself re-fetches the price
// from the venue in-TEE and decides — so the poller cannot lie about the
// price, cannot widen the bounds, and cannot make it sell below the floor.
// An untriggered tick costs exactly one outbound fetch and changes nothing.
//
// Semantics: protective stop for a LONG. When last <= stopPrice, market-sell
// `amount` — but only when every fence holds:
//   - floorPrice  never sell below this (gap-down protection: if the market
//                 gapped through the floor, REFUSE and report instead of
//                 dumping into a crash)
//   - maxAmount   hard cap on the sellable amount
//   - idempotency a clientOrderId derived deterministically from triggerId
//                 dedupes concurrent double-fires venue-side; on spot the
//                 sell is also clamped to the free base balance, and on
//                 hyperliquid reduceOnly:true makes a re-fire on a flat
//                 position structurally unable to oversell
//
// REQUIRES the lit-venues IIFE bundle concatenated ABOVE this file (global
// `LitVenues`) — scripts/_lit.js composes `bundle + "\n" + this file`.
//
// js_params (sizes/prices are DECIMAL STRINGS — no floats in the policy path):
//   venueId      "binance" (default) | "binanceus" | "coinbase" | "hyperliquid"
//   sandbox      default true (binance spot testnet / hyperliquid testnet)
//   apiKey, secret, keyType        CEX credentials
//   privateKey, accountAddress     hyperliquid (pkp-eip712); or useActionKey:
//   useActionKey true to sign with the action's CID-bound TEE key
//   proxyUrl     optional egress proxy (plan D4)
//   symbol       unified symbol, e.g. "BTC/USDT"
//   stopPrice    trigger: act when last <= stopPrice
//   amount       base amount to sell when triggered
//   floorPrice   POLICY: never sell below this price
//   maxAmount    POLICY: hard ceiling on amount
//   triggerId    idempotency seed for the derived clientOrderId
//   market       optional pre-fetched Market (markets-cache injection: saves
//                the fetchMarket round-trip on the triggered path)
//   dryRun       evaluate everything but place nothing
//
// Returns { triggered, order? } plus diagnostics.

const main = async (params) => {
  // Setup probe: confirms the usage-key grant has propagated and that the
  // concatenated lit-venues bundle parses + executes inside the runtime.
  if (params.probe) return { ok: true, note: "probe", litVenuesVersion: LitVenues.VERSION };

  // Exact decimal-string compare via lit-venues' scaled-BigInt subtraction.
  const cmpDec = (a, b) => {
    const d = LitVenues.subDec(a, b);
    return d === "0" ? 0 : d[0] === "-" ? -1 : 1;
  };
  const minDec = (a, b) => (cmpDec(a, b) <= 0 ? a : b);

  // ---- policy params — fixed at arm time (see README "Trust model") -------
  for (const k of ["symbol", "stopPrice", "amount", "floorPrice", "maxAmount", "triggerId"]) {
    if (params[k] === undefined || params[k] === null || params[k] === "") {
      throw new Error(`required param missing: ${k}`);
    }
  }
  const venueId = params.venueId || "binance";
  const symbol = params.symbol;
  const stopPrice = LitVenues.wireDecimal(String(params.stopPrice));
  const floorPrice = LitVenues.wireDecimal(String(params.floorPrice));
  const amount = LitVenues.wireDecimal(String(params.amount));
  const maxAmount = LitVenues.wireDecimal(String(params.maxAmount));
  if (cmpDec(floorPrice, stopPrice) > 0) {
    throw new Error(`policy misconfigured: floorPrice ${floorPrice} must be <= stopPrice ${stopPrice}`);
  }
  if (cmpDec(amount, maxAmount) > 0) {
    throw new Error(`policy violation: amount ${amount} exceeds maxAmount ${maxAmount} — refusing`);
  }

  // Deterministic idempotency key from triggerId, so a double-fire reuses the
  // SAME clientOrderId.
  //   hyperliquid: cloid must match /^0x[0-9a-f]{32}$/ (128-bit hex). UUID-ish
  //   ids (32+ hex chars once dashes are stripped) are used directly; anything
  //   else is hex-encoded per character, then zero-padded to 32 hex chars.
  //   spot venues: <=36 chars of [A-Za-z0-9_-].
  const deriveClientOrderId = (id) => {
    const s = String(id);
    if (venueId === "hyperliquid") {
      const hexish = s.toLowerCase().replace(/-/g, "");
      if (/^[0-9a-f]{32,}$/.test(hexish)) return "0x" + hexish.slice(0, 32);
      let hex = "";
      for (let i = 0; i < s.length && hex.length < 32; i++) hex += s.charCodeAt(i).toString(16).padStart(2, "0");
      return "0x" + hex.padEnd(32, "0").slice(0, 32);
    }
    return ("stop-" + s.replace(/[^A-Za-z0-9_-]/g, "")).slice(0, 36);
  };

  // ---- venue client --------------------------------------------------------
  let credentials;
  if (venueId === "hyperliquid") {
    credentials = {
      keyType: "pkp-eip712",
      privateKey: params.useActionKey ? await Lit.Actions.getLitActionPrivateKey() : params.privateKey,
      accountAddress: params.accountAddress || undefined,
    };
  } else {
    credentials = { apiKey: params.apiKey, secret: params.secret, keyType: params.keyType || "hmac" };
  }
  const venue = LitVenues.createVenue({
    venueId,
    sandbox: params.sandbox !== false,
    credentials,
    proxy: params.proxyUrl || undefined,
    // Markets-cache injection: a pre-fetched Market makes fetchMarket() below
    // answer without an HTTP round-trip on the triggered path.
    markets: params.market ? { [symbol]: params.market } : undefined,
  });

  // ---- the price check — in-TEE, from the venue itself ---------------------
  const ticker = await venue.fetchTicker(symbol); // 1 fetch — the ONLY one on an untriggered tick
  const last = LitVenues.wireDecimal(String(ticker.last));

  if (cmpDec(last, stopPrice) > 0) {
    return { triggered: false, last, stopPrice };
  }

  // === stop triggered ========================================================

  // Fence: the floor. If the market gapped below it, refuse — a stop is
  // protection, not an obligation to sell into a crash.
  if (cmpDec(last, floorPrice) < 0) {
    return {
      triggered: true,
      sold: false,
      refused: `last ${last} is below floorPrice ${floorPrice} — refusing to market-sell below the floor`,
      last,
      floorPrice,
    };
  }

  // Fence: idempotency — if our derived clientOrderId is already resting,
  // a previous fire placed it; do nothing.
  const clientOrderId = deriveClientOrderId(params.triggerId);
  const open = await venue.fetchOpenOrders(symbol); // 1 fetch
  if (open.some((o) => o.clientOrderId === clientOrderId)) {
    return { triggered: true, sold: false, alreadyPlaced: true, clientOrderId, last };
  }

  // Size exactly on the venue's lot grid, capped by policy.
  const market = await venue.fetchMarket(symbol); // 0 fetches if params.market injected, else 1
  let sellable = minDec(amount, maxAmount);
  if (venueId !== "hyperliquid") {
    // Spot: clamp to the free base balance, so a sequential re-fire after a
    // fill (clientOrderId no longer open) finds nothing left to sell.
    const balances = await venue.fetchBalances(); // 1 fetch
    const base = balances.find((b) => b.asset === market.base);
    sellable = minDec(sellable, (base && base.free) || "0");
  }
  const amt = LitVenues.roundDownToIncrement(sellable, market.amountIncrement);
  if (cmpDec(amt, "0") <= 0) {
    return { triggered: true, sold: false, note: "nothing to sell (position already flat?) — disarm the trigger", last };
  }

  const req = { symbol, side: "sell", type: "market", amount: amt, clientOrderId };
  if (venueId === "hyperliquid") {
    // Perps: the order may only REDUCE the long — a duplicate fire on a flat
    // position is rejected by the venue itself. Spot venues reject the flag.
    req.reduceOnly = true;
  }

  if (params.dryRun) {
    return { triggered: true, sold: false, dryRun: true, wouldPlace: req, last };
  }

  const order = await venue.createOrder(req); // 1 fetch (+1 internal mids fetch on hyperliquid)
  return {
    triggered: true,
    sold: true,
    last,
    order: {
      id: order.id,
      clientOrderId: order.clientOrderId,
      status: order.status,
      amount: order.amount,
      filled: order.filled,
      price: order.price,
    },
  };
};
