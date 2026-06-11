// Lit Action — a Hyperliquid perp trader whose key never exists anywhere
// (plan D8, the PKP-native showcase).
//
// This file is NOT submitted alone: scripts/_lit.js concatenates the prebuilt
// lit-venues IIFE bundle (../../lit-venues/dist/lit-venues.iife.js) above it,
// defining the global `LitVenues`. `Lit.Actions` comes from the runtime.
//
// The trading key is `Lit.Actions.getLitActionPrivateKey()` — a secp256k1 key
// derived from this action's IPFS CID that never leaves the Lit TEE. The CID
// covers the bundle AND this source, so the key — and the agent address
// derived from it — is bound to this exact code, POLICY constants included.
// Change a byte (or rebuild the bundle) and it's a different key: the old
// venue-side agent approval simply stops matching anything that can sign.
//
// The user's master account signs ONE `approveAgent` for that address
// (scripts/approve-agent.js). From then on this action signs orders as the
// agent; Hyperliquid structurally bars agents from withdrawals and transfers,
// so the venue itself enforces that this code can trade but never exfiltrate.
// Custody stays with the master, which can revoke the agent venue-side at any
// time.
//
// Two operations, selected by `action`:
//   action: "address" -> return the agent address (run at setup/connect time)
//   action: "trade"   -> enforce POLICY below, then place the order
//
// POLICY is part of the hashed source: the fences below are bound to the
// agent address exactly like the key is.
//
// js_params (trade):
//   masterAddress  the master account the agent was approved for (0x...)
//   coin           "ETH" | "BTC" (POLICY.allowedCoins)
//   side           "buy" | "sell"
//   amount         base size, decimal string (e.g. "0.01")
//   price          decimal string -> GTC limit; omit -> market (IOC vs mid)
//   leverage       integer, clamped to POLICY.maxLeverage
//   reduceOnly     true -> the order may only shrink the position
//   clientOrderId  optional cloid, must match /^0x[0-9a-f]{32}$/

const POLICY = {
  allowedCoins: ["ETH", "BTC"],
  maxLeverage: 3, // requested leverage is clamped here, cross margin
  maxNotionalUsd: 1000, // cap on order notional AND resulting position notional
  sandbox: true, // testnet-only as written: this is bound into the CID too
};

async function main(params) {
  const privateKey = await Lit.Actions.getLitActionPrivateKey();
  const agentAddress = LitVenues.privateKeyToAddress(privateKey);

  if (params && params.action === "address") {
    return { agentAddress, litVenuesVersion: LitVenues.VERSION };
  }
  if (!params || params.action !== "trade") {
    return { authorized: false, reason: `unknown action "${params && params.action}"` };
  }

  // ---- Validate the request shape ------------------------------------------
  const { masterAddress, coin, side, amount, price, clientOrderId } = params;
  if (typeof masterAddress !== "string" || !/^0x[0-9a-fA-F]{40}$/.test(masterAddress)) {
    return { authorized: false, reason: "masterAddress must be a 0x address (run approve-agent first)" };
  }
  if (!POLICY.allowedCoins.includes(coin)) {
    return { authorized: false, reason: `coin "${coin}" not in policy allowlist ${JSON.stringify(POLICY.allowedCoins)}` };
  }
  if (side !== "buy" && side !== "sell") {
    return { authorized: false, reason: `side must be "buy" or "sell", got "${side}"` };
  }
  if (typeof amount !== "string" || !/^\d+(\.\d+)?$/.test(amount) || Number(amount) <= 0) {
    return { authorized: false, reason: "amount must be a positive decimal string" };
  }
  if (price !== undefined && price !== null && (typeof price !== "string" || !/^\d+(\.\d+)?$/.test(price))) {
    return { authorized: false, reason: "price, when given, must be a decimal string" };
  }
  if (clientOrderId && !/^0x[0-9a-f]{32}$/.test(clientOrderId)) {
    return { authorized: false, reason: "clientOrderId (cloid) must match /^0x[0-9a-f]{32}$/" };
  }

  // The agent signs; reads and trades are against the MASTER account.
  const hl = LitVenues.createVenue({
    venueId: "hyperliquid",
    sandbox: POLICY.sandbox,
    credentials: { keyType: "pkp-eip712", privateKey, accountAddress: masterAddress },
  });

  // ---- Fences, BEFORE anything is signed ------------------------------------
  // Order fields stay decimal strings end to end (lit-venues invariant); the
  // Number() math below only powers the policy *gate*, never an order field.
  const symbol = coin; // lit-venues accepts the bare coin for hyperliquid
  const refPrice = price ? Number(price) : (await hl.fetchTicker(symbol)).last;
  const orderNotional = Number(amount) * refPrice;

  const positionsBefore = await hl.fetchPositions();
  const pos = positionsBefore.find((p) => p.symbol === coin);
  const posSize = pos ? Number(pos.size) : 0; // signed: negative = short
  const orderSign = side === "buy" ? 1 : -1;
  const resultingSize = posSize + orderSign * Number(amount);
  const resultingNotional = Math.abs(resultingSize) * refPrice;

  // Reduce-only first: a reduce-only order can never increase exposure (the
  // venue enforces that), so it bypasses the notional caps below.
  let reduceOnly = params.reduceOnly === true;
  if (!reduceOnly && orderSign * posSize < 0 && resultingNotional > POLICY.maxNotionalUsd) {
    // The order opposes the position but would overshoot past flat into a
    // breach: force reduce-only so it can shrink the position, never flip it.
    reduceOnly = true;
  }
  if (!reduceOnly) {
    if (orderNotional > POLICY.maxNotionalUsd) {
      return {
        authorized: false,
        reason: `order notional ~$${orderNotional.toFixed(2)} exceeds policy cap $${POLICY.maxNotionalUsd}`,
      };
    }
    if (resultingNotional > POLICY.maxNotionalUsd) {
      // Same-direction growth past the cap: there is nothing to reduce.
      return {
        authorized: false,
        reason:
          `resulting position notional ~$${resultingNotional.toFixed(2)} would exceed ` +
          `policy cap $${POLICY.maxNotionalUsd} (current size ${posSize} ${coin})`,
      };
    }
  }

  const requestedLeverage = Number.isInteger(params.leverage) && params.leverage >= 1 ? params.leverage : 1;
  const leverage = Math.min(requestedLeverage, POLICY.maxLeverage);

  // ---- Trade, inside the fences ---------------------------------------------
  await hl.setLeverage(symbol, leverage, { cross: true });
  const order = await hl.createOrder({
    symbol,
    side,
    type: price ? "limit" : "market", // market = aggressive IOC limit vs mid
    amount,
    price: price || undefined,
    timeInForce: price ? "GTC" : undefined,
    clientOrderId: clientOrderId || undefined,
    reduceOnly,
  });
  const positions = await hl.fetchPositions();

  return {
    authorized: true,
    agentAddress,
    policy: {
      leverage,
      leverageRequested: requestedLeverage,
      reduceOnly,
      orderNotionalUsd: Math.round(orderNotional * 100) / 100,
      maxNotionalUsd: POLICY.maxNotionalUsd,
    },
    order,
    positions,
  };
}
