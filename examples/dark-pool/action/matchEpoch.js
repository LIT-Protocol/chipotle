// Lit Action: the heart of the dark pool. Closes one epoch's batch.
//
//   1. decrypt the DB connection string (an encrypted secret),
//   2. SELECT the epoch's open orders (ciphertext) over Neon SQL-over-HTTP,
//   3. Decrypt every order INSIDE the enclave,
//   4. AUTHENTICATE each order: verify the trader's signature, reject foreign /
//      malformed / duplicate orders (this is what stops anyone with the usage
//      key from forging an order for someone else's escrow),
//   5. run a uniform-price sealed-bid call auction (one clearing price) over the
//      authenticated orders,
//   6. sign the resulting fills with the action's CID-derived key,
//   7. return the fills + signature for on-chain settlement.
//
// Order contents are visible only here, in TEE memory, for the duration of the
// match. The connection string and the decrypted orders are NEVER returned or
// logged. Provider error bodies are NOT echoed (they could carry request data).
//
// UNITS (must match DarkPoolSettlement.sol exactly):
//   quantity   = base smallest-units            (uint256, < 2^128)
//   limitPrice = quote-units per base-unit x1e18 (uint256, < 2^128)
//   clearingPx = same scale as limitPrice
//   quoteCost  = quantity * clearingPx / 1e18    (computed on-chain)
//
// PER-ORDER TRADER SIGNATURE (must match submitOrder.js):
//   orderDigest = keccak256(abi.encode(
//     uint256 chainId, address settlement, uint256 epoch, bytes32 keccak256(pair),
//     bool isBuy, uint256 limitPrice, uint256 quantity, uint256 nonce))
//   sig = personal_sign(orderDigest) by the trader
//
// SETTLEMENT DIGEST (must match DarkPoolSettlement.settleEpoch exactly):
//   inner = keccak256(abi.encode(
//     uint256 epoch, bytes32 keccak256(pair), uint256 clearingPx,
//     bytes32 keccak256(abi.encode(Fill[])), address settlement, uint256 chainId))
//   signature = personal_sign(inner)
//   Fill = tuple(address trader, bool isBuy, uint256 quantity)
//
// js_params:
//   pkpId, encryptedDbUrl, epoch, pair, settlement, chainId, maxBatch

const MAX_UINT128 = 2n ** 128n;

async function main({ pkpId, encryptedDbUrl, epoch, pair, settlement, chainId, maxBatch }) {
  const dbUrl = await Lit.Actions.Decrypt({ pkpId, ciphertext: encryptedDbUrl });

  const cap = Number(maxBatch) > 0 ? Number(maxBatch) : 200;
  // Fetch cap+1 so an over-cap book actually trips the guard below (a plain
  // LIMIT cap can never exceed cap, so the check would be dead code).
  const rows = (
    await neonQuery(
      dbUrl,
      "select id, ciphertext from orders where epoch = $1 and pair = $2 and not settled order by id asc limit $3",
      [epoch, pair, cap + 1]
    )
  ).rows;

  if (rows.length > cap) {
    // Never silently match a partial book and strand the rest in a settle-once epoch.
    throw new Error(`epoch ${epoch} exceeds maxBatch (${cap}); refuse to match a partial book`);
  }

  const decoded = [];
  for (const row of rows) {
    const plain = await Lit.Actions.Decrypt({ pkpId, ciphertext: row.ciphertext });
    const o = JSON.parse(plain);
    decoded.push({
      id: Number(row.id),
      side: o.side,
      limitPrice: safeBig(o.limitPrice),
      quantity: safeBig(o.quantity),
      trader: o.trader,
      nonce: o.nonce,
      sig: o.sig,
    });
  }

  // AUTH: keep only orders the named trader actually signed for this epoch/pool.
  const { accepted, rejected } = authenticateOrders(decoded, { chainId, settlement, epoch, pair });

  const { clearingPx, fills } = runAuction(accepted);

  // Sign the fills for on-chain settlement (CID-derived key).
  const pairHash = ethers.utils.keccak256(ethers.utils.toUtf8Bytes(pair));
  const fillTuples = fills.map((f) => [f.trader, f.isBuy, f.quantity.toString()]);
  const fillsHash = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["tuple(address trader, bool isBuy, uint256 quantity)[]"],
      [fillTuples]
    )
  );
  const inner = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["uint256", "bytes32", "uint256", "bytes32", "address", "uint256"],
      [String(epoch), pairHash, clearingPx.toString(), fillsHash, settlement, String(chainId)]
    )
  );
  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const signature = await wallet.signMessage(ethers.utils.arrayify(inner));

  return {
    epoch,
    pair,
    clearingPx: clearingPx.toString(),
    fills: fills.map((f) => ({ trader: f.trader, isBuy: f.isBuy, quantity: f.quantity.toString() })),
    orderIds: accepted.map((o) => o.id), // authenticated orders settled this epoch
    matchedOrders: accepted.length,
    rejectedOrders: rejected.length,
    signer: wallet.address,
    signature,
  };
}

// ---------------------------------------------------------------------------
// Authentication. Reject any order whose trader signature is missing, invalid,
// for a different trader/epoch/pool, out of range, or a (trader,nonce) replay.
// Pure (ethers only) — unit-tested in test/auth.test.js.
// ---------------------------------------------------------------------------
function authenticateOrders(orders, ctx) {
  const pairHash = ethers.utils.keccak256(ethers.utils.toUtf8Bytes(ctx.pair));
  const seen = new Set();
  const accepted = [];
  const rejected = [];
  for (const o of orders) {
    const reason = orderRejectReason(o, ctx, pairHash, seen);
    if (reason) {
      rejected.push({ id: o.id, reason });
      continue;
    }
    seen.add(o.trader.toLowerCase() + ":" + String(o.nonce));
    accepted.push(o);
  }
  return { accepted, rejected };
}

function orderRejectReason(o, ctx, pairHash, seen) {
  if (o.side !== "buy" && o.side !== "sell") return "bad side";
  if (!(o.limitPrice > 0n) || o.limitPrice >= MAX_UINT128) return "limitPrice out of range";
  if (!(o.quantity > 0n) || o.quantity >= MAX_UINT128) return "quantity out of range";
  if (typeof o.trader !== "string" || !/^0x[0-9a-fA-F]{40}$/.test(o.trader)) return "bad trader";
  if (o.nonce === undefined || o.nonce === null || !o.sig) return "missing nonce/signature";
  const key = o.trader.toLowerCase() + ":" + String(o.nonce);
  if (seen.has(key)) return "duplicate (trader, nonce)";
  let recovered;
  try {
    const digest = orderDigest(ctx.chainId, ctx.settlement, ctx.epoch, pairHash, o.side === "buy", o.limitPrice, o.quantity, o.nonce);
    recovered = ethers.utils.verifyMessage(ethers.utils.arrayify(digest), o.sig);
  } catch (e) {
    return "unverifiable signature";
  }
  if (recovered.toLowerCase() !== o.trader.toLowerCase()) return "signature does not match trader";
  return null;
}

function orderDigest(chainId, settlement, epoch, pairHash, isBuy, limitPrice, quantity, nonce) {
  return ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["uint256", "address", "uint256", "bytes32", "bool", "uint256", "uint256", "uint256"],
      [String(chainId), settlement, String(epoch), pairHash, isBuy, limitPrice.toString(), quantity.toString(), String(nonce)]
    )
  );
}

function safeBig(v) {
  // Tolerate junk in a decrypted blob without throwing; out-of-range values are
  // rejected by orderRejectReason. Returns -1n for anything non-numeric.
  try {
    return BigInt(v);
  } catch {
    return -1n;
  }
}

// ---------------------------------------------------------------------------
// Pure uniform-price call auction. No I/O — unit-tested in test/auction.test.js.
//
// Returns { clearingPx: BigInt, fills: [{trader, isBuy, quantity: BigInt}] }.
// Guarantees sum(buy fills) === sum(sell fills) === matched volume, so
// DarkPoolSettlement's conservation check always passes.
// ---------------------------------------------------------------------------
function runAuction(orders) {
  const buysAll = orders.filter((o) => o.side === "buy");
  const sellsAll = orders.filter((o) => o.side === "sell");

  const priceSet = new Set(orders.map((o) => o.limitPrice.toString()));
  let best = null; // { px, vol, imbalance }
  for (const s of priceSet) {
    const px = BigInt(s);
    let demand = 0n;
    let supply = 0n;
    for (const o of buysAll) if (o.limitPrice >= px) demand += o.quantity;
    for (const o of sellsAll) if (o.limitPrice <= px) supply += o.quantity;
    const vol = demand < supply ? demand : supply;
    const imbalance = demand > supply ? demand - supply : supply - demand;
    if (
      best === null ||
      vol > best.vol ||
      (vol === best.vol && imbalance < best.imbalance) ||
      (vol === best.vol && imbalance === best.imbalance && px < best.px)
    ) {
      best = { px, vol, imbalance };
    }
  }

  if (best === null || best.vol === 0n) {
    return { clearingPx: 0n, fills: [] };
  }

  const px = best.px;
  const V = best.vol;
  const buys = buysAll.filter((o) => o.limitPrice >= px);
  const sells = sellsAll.filter((o) => o.limitPrice <= px);
  const buyTotal = buys.reduce((acc, o) => acc + o.quantity, 0n);
  const sellTotal = sells.reduce((acc, o) => acc + o.quantity, 0n);

  let buyFilled;
  let sellFilled;
  if (buyTotal <= sellTotal) {
    buyFilled = buys.map((o) => ({ o, q: o.quantity }));
    sellFilled = ration(sells, sellTotal, V);
  } else {
    sellFilled = sells.map((o) => ({ o, q: o.quantity }));
    buyFilled = ration(buys, buyTotal, V);
  }

  const fills = [];
  for (const { o, q } of buyFilled) if (q > 0n) fills.push({ trader: o.trader, isBuy: true, quantity: q });
  for (const { o, q } of sellFilled) if (q > 0n) fills.push({ trader: o.trader, isBuy: false, quantity: q });
  return { clearingPx: px, fills };
}

// Floor + largest-remainder so allocations sum to EXACTLY `target`
// (deterministic: remainder desc, then order id asc).
function ration(side, total, target) {
  if (target === total) return side.map((o) => ({ o, q: o.quantity }));
  const allocs = side.map((o) => {
    const num = o.quantity * target;
    return { o, q: num / total, rem: num % total };
  });
  let assigned = allocs.reduce((acc, a) => acc + a.q, 0n);
  let leftover = target - assigned;
  allocs.sort((a, b) => {
    if (a.rem !== b.rem) return a.rem > b.rem ? -1 : 1;
    return a.o.id - b.o.id;
  });
  for (let i = 0; i < allocs.length && leftover > 0n; i++) {
    allocs[i].q += 1n;
    leftover -= 1n;
  }
  return allocs;
}

// Minimal Neon "SQL over HTTP" client. Error bodies are NOT included in the
// thrown message — they can echo request data; only the status code is surfaced.
async function neonQuery(connectionString, query, params) {
  const host = new URL(connectionString).host;
  const res = await fetch(`https://${host}/sql`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Neon-Connection-String": connectionString,
      "Neon-Array-Mode": "true",
    },
    body: JSON.stringify({ query, params: params || [] }),
  });
  if (!res.ok) {
    throw new Error(`neon sql request failed (status ${res.status})`);
  }
  const body = await res.json();
  const names = (body.fields || []).map((f) => f.name);
  return {
    rows: (body.rows || []).map((r) => Object.fromEntries(names.map((n, i) => [n, r[i]]))),
    rowCount: body.rowCount,
  };
}
