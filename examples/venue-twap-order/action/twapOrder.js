// Lit Action: ONE TWAP tick — the D7 "chained triggers, not long actions"
// pattern (plans/ccxt-venue-layer-and-email-approval.md).
//
// A TWAP is not one long-running action: it's a cron trigger + this small
// action once per tick + strategy state passed back by the caller. Each call
// places AT MOST ONE child order, inside hard policy fences, and returns the
// updated state for the caller to persist and pass into the next tick. Every
// tick is an independently attested, auditable execution that stays far
// inside the per-action fetch quota.
//
// REQUIRES the lit-venues IIFE bundle concatenated ABOVE this file (global
// `LitVenues`) — scripts/_lit.js composes `bundle + "\n" + this file`, the
// same inline-bundle pattern as e2e/tests/api/lit-venues-spike.spec.ts.
//
// js_params (all sizes/prices are DECIMAL STRINGS — floats never touch the
// policy path; see the exact-math helpers below):
//   state            null on the first tick; thereafter the state returned by
//                    the previous tick, passed back verbatim by the caller
//   venueId          "binance" (default) | "binanceus" | "coinbase" | "hyperliquid"
//   sandbox          default true (binance spot testnet / hyperliquid testnet)
//   apiKey, secret, keyType        CEX credentials (binance: keyType "hmac")
//   privateKey, accountAddress     hyperliquid (keyType pkp-eip712); or set
//   useActionKey     true to trade with the action's own CID-bound TEE key
//                    (Lit.Actions.getLitActionPrivateKey()) on hyperliquid
//   proxyUrl         optional egress proxy (plan D4) for geo-blocked venues
//   symbol           unified symbol, e.g. "BTC/USDT"
//   side             "buy" (default) | "sell"
//   totalAmount      total base amount to execute across all slices
//   slices           number of child orders to split totalAmount into
//   maxSliceNotional POLICY: abort the slice if amount*last exceeds this
//   maxDriftBps      POLICY: abort the slice if last moved more than this many
//                    bps (either direction) from the first tick's reference price
//   orderType        "market" (default) | "limit" (aggressive IOC limit)
//   limitOffsetBps   how far through the touch the aggressive limit prices
//                    itself (default 50)
//   maxTicks         expiry fence: give up after this many ticks (default slices*5)
//   twapId           short id used to derive per-slice clientOrderIds (audit +
//                    open-order dedup; see README for what that does NOT guarantee)
//   dryRun           evaluate every fence but do not place the order
//
// Returns { state, ... } — the caller MUST persist `state` and pass it back
// on the next tick (or seal it with Lit.Actions.Encrypt — see README).

const main = async (params) => {
  // Setup probe: confirms the usage-key grant has propagated and that the
  // concatenated lit-venues bundle parses + executes inside the runtime.
  if (params.probe) return { ok: true, note: "probe", litVenuesVersion: LitVenues.VERSION };

  // ---- exact decimal-string math (scaled BigInt — no floats, ever) -------
  // lit-venues exports addDec/subDec/roundDownToIncrement/applyBps; the two
  // helpers it doesn't export (compare, multiply, divide-by-count) are
  // implemented here the same way: scaled BigInts over decimal strings.
  const cmpDec = (a, b) => {
    const d = LitVenues.subDec(a, b);
    return d === "0" ? 0 : d[0] === "-" ? -1 : 1;
  };
  const mulDec = (a, b) => {
    const parts = (s) => {
      const neg = s[0] === "-";
      const t = neg ? s.slice(1) : s;
      const dot = t.indexOf(".");
      const digits = dot < 0 ? t : t.slice(0, dot) + t.slice(dot + 1);
      return { v: BigInt(digits || "0") * (neg ? -1n : 1n), dp: dot < 0 ? 0 : t.length - dot - 1 };
    };
    const x = parts(a);
    const y = parts(b);
    const prod = x.v * y.v;
    const dp = x.dp + y.dp;
    const abs = (prod < 0n ? -prod : prod).toString().padStart(dp + 1, "0");
    const int = abs.slice(0, abs.length - dp) || "0";
    const frac = dp ? abs.slice(abs.length - dp) : "";
    return LitVenues.wireDecimal((prod < 0n ? "-" : "") + int + (frac ? "." + frac : ""));
  };
  // Floor-divide a positive decimal string by an integer count, at dp decimals.
  const divDecByInt = (value, n, dp) => {
    const dot = value.indexOf(".");
    const int = dot < 0 ? value : value.slice(0, dot);
    const frac = (dot < 0 ? "" : value.slice(dot + 1)).padEnd(dp, "0").slice(0, dp);
    const q = BigInt((int || "0") + frac) / BigInt(n);
    const s = q.toString().padStart(dp + 1, "0");
    return LitVenues.wireDecimal(s.slice(0, s.length - dp) + (dp ? "." + s.slice(s.length - dp) : ""));
  };

  // ---- params + policy bounds --------------------------------------------
  const venueId = params.venueId || "binance";
  for (const k of ["symbol", "totalAmount", "slices", "maxSliceNotional", "maxDriftBps"]) {
    if (params[k] === undefined || params[k] === null || params[k] === "") {
      throw new Error(`required param missing: ${k}`);
    }
  }
  const symbol = params.symbol;
  const side = params.side === "sell" ? "sell" : "buy";
  const slices = Number(params.slices);
  if (!Number.isInteger(slices) || slices < 1 || slices > 500) throw new Error("slices must be an integer in 1..500");
  const maxDriftBps = Number(params.maxDriftBps);
  if (!Number.isInteger(maxDriftBps) || maxDriftBps < 0) throw new Error("maxDriftBps must be a non-negative integer");
  const totalAmount = LitVenues.wireDecimal(String(params.totalAmount));
  const maxSliceNotional = LitVenues.wireDecimal(String(params.maxSliceNotional));
  const orderType = params.orderType === "limit" ? "limit" : "market";
  const limitOffsetBps = params.limitOffsetBps === undefined ? 50 : Number(params.limitOffsetBps);

  let state = params.state || null;
  if (state && state.done) return { state, note: "strategy already complete — nothing to do" };

  // ---- venue client -------------------------------------------------------
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
    // Markets-cache injection (plan M1): after the first tick the market rules
    // ride in `state`, so fetchMarket() below answers WITHOUT an HTTP fetch.
    markets: state && state.market ? { [symbol]: state.market } : undefined,
  });

  const market = await venue.fetchMarket(symbol); // tick 1: 1 fetch; tick 2+: 0 (injected)
  const ticker = await venue.fetchTicker(symbol); // 1 fetch
  const last = LitVenues.wireDecimal(String(ticker.last));

  if (!state) {
    state = {
      referencePrice: last, // the first tick's price anchors the drift band
      remaining: totalAmount,
      filledSlices: 0,
      ticks: 0,
      orders: [],
      market, // carried so later ticks inject it (see `markets` above)
      done: false,
    };
  }
  state.ticks += 1;
  state.market = market;

  // ---- fence: expiry — a cron must not run forever ------------------------
  const maxTicks = params.maxTicks ? Number(params.maxTicks) : slices * 5;
  if (state.ticks > maxTicks) {
    state.done = true;
    return { state, aborted: true, reason: `expired: ${state.ticks} ticks > maxTicks ${maxTicks} with ${state.filledSlices}/${slices} slices placed` };
  }

  // ---- fence: hard price band vs the first tick's reference price ---------
  const bandDp = LitVenues.decimalsOf(state.referencePrice) + 4;
  const lower = LitVenues.applyBps(state.referencePrice, -maxDriftBps, bandDp);
  const upper = LitVenues.applyBps(state.referencePrice, maxDriftBps, bandDp);
  if (cmpDec(last, lower) < 0 || cmpDec(last, upper) > 0) {
    return {
      state,
      skipped: true,
      reason: `price ${last} is outside the ±${maxDriftBps}bps band [${lower}, ${upper}] around reference ${state.referencePrice} — slice aborted`,
    };
  }

  // ---- size this slice exactly on the venue's lot grid --------------------
  const inc = market.amountIncrement;
  const slicesLeft = slices - state.filledSlices;
  const sliceAmount =
    slicesLeft <= 1
      ? LitVenues.roundDownToIncrement(state.remaining, inc) // last slice mops the remainder
      : LitVenues.roundDownToIncrement(divDecByInt(state.remaining, slicesLeft, LitVenues.decimalsOf(inc) + 2), inc);
  if (cmpDec(sliceAmount, "0") <= 0) {
    state.done = true;
    return { state, note: "remaining amount is dust below the venue lot size — strategy complete" };
  }
  if (market.minAmount && cmpDec(sliceAmount, market.minAmount) < 0) {
    state.done = true;
    return { state, aborted: true, reason: `slice amount ${sliceAmount} is below the venue minimum ${market.minAmount} — size totalAmount/slices larger` };
  }

  // ---- fence: max per-slice notional --------------------------------------
  const notional = mulDec(sliceAmount, last);
  if (cmpDec(notional, maxSliceNotional) > 0) {
    return { state, skipped: true, reason: `slice notional ${notional} exceeds maxSliceNotional ${maxSliceNotional} — slice aborted` };
  }
  if (market.minNotional && cmpDec(notional, market.minNotional) < 0) {
    state.done = true;
    return { state, aborted: true, reason: `slice notional ${notional} is below the venue minNotional ${market.minNotional} — size totalAmount/slices larger` };
  }

  // ---- build the single child order ---------------------------------------
  if (venueId === "coinbase" && orderType === "market" && side === "buy") {
    // Advanced Trade market BUYs are quote-sized (quoteAmount); this TWAP is
    // base-sized by design. Use orderType "limit" on coinbase instead.
    throw new Error('coinbase market BUY takes quoteAmount, not a base amount — set orderType:"limit" for coinbase TWAP buys');
  }
  const req = { symbol, side, type: orderType, amount: sliceAmount };
  if (orderType === "limit") {
    // Aggressive (marketable) limit: price through the last trade by
    // limitOffsetBps, rounded onto the venue tick grid, IOC so any unfilled
    // remainder cancels instead of resting.
    const px = LitVenues.applyBps(last, side === "buy" ? limitOffsetBps : -limitOffsetBps, LitVenues.decimalsOf(market.priceIncrement) + 4);
    req.price = LitVenues.roundDownToIncrement(px, market.priceIncrement);
    req.timeInForce = "IOC";
  }
  if (params.twapId) {
    const seed = `${params.twapId}:${state.filledSlices}`;
    if (venueId === "hyperliquid") {
      // hyperliquid cloid must match /^0x[0-9a-f]{32}$/ — hex-encode the seed
      // chars and zero-pad to exactly 32 hex chars (128 bits), deterministically.
      let hex = "";
      for (let i = 0; i < seed.length && hex.length < 32; i++) hex += seed.charCodeAt(i).toString(16).padStart(2, "0");
      req.clientOrderId = "0x" + hex.padEnd(32, "0").slice(0, 32);
    } else {
      req.clientOrderId = ("twap-" + seed.replace(/[^A-Za-z0-9_-]/g, "-")).slice(0, 36);
    }
  }

  if (params.dryRun) {
    return { state, dryRun: true, sliceIndex: state.filledSlices, notional, last, wouldPlace: req };
  }

  const order = await venue.createOrder(req); // 1 fetch (+1 internal mids fetch for hyperliquid market orders)

  // Decrement by the venue-reported fill when present (IOC may partially
  // fill); otherwise by the requested slice. Unfilled remainder stays in
  // `remaining`, so later slices self-correct.
  const executed = order.filled && cmpDec(LitVenues.wireDecimal(order.filled), "0") > 0 ? LitVenues.wireDecimal(order.filled) : sliceAmount;
  state.remaining = LitVenues.subDec(state.remaining, executed);
  state.filledSlices += 1;
  state.orders.push({
    slice: state.filledSlices,
    id: order.id,
    clientOrderId: order.clientOrderId,
    status: order.status,
    amount: executed,
    price: order.price,
    last,
    ts: ticker.ts,
  });
  state.done = state.filledSlices >= slices || cmpDec(LitVenues.roundDownToIncrement(state.remaining, inc), "0") <= 0;

  return {
    state,
    placed: { slice: state.filledSlices, of: slices, id: order.id, status: order.status, amount: executed, notional, last },
    done: state.done,
  };
};
