// Lit Action: the heart of the dark pool. Closes one epoch's batch.
//
//   1. decrypt the DB connection string (an encrypted secret),
//   2. SELECT the epoch's open orders (ciphertext) over Neon SQL-over-HTTP,
//   3. Decrypt every order INSIDE the enclave,
//   4. run a uniform-price sealed-bid call auction (one clearing price),
//   5. sign the resulting fills with the action's CID-derived key,
//   6. return the fills + signature for on-chain settlement.
//
// Order contents are visible only here, in TEE memory, for the duration of the
// match. The connection string and the decrypted orders are NEVER returned or
// logged.
//
// UNITS (must match DarkPoolSettlement.sol exactly):
//   quantity   = base smallest-units            (uint256)
//   limitPrice = quote-units per base-unit x1e18 (uint256)
//   clearingPx = same scale as limitPrice
//   quoteCost  = quantity * clearingPx / 1e18    (computed on-chain)
//
// SIGNED DIGEST (must match DarkPoolSettlement.settleEpoch exactly):
//   inner = keccak256(abi.encode(
//             uint256 epoch, bytes32 keccak256(pair), uint256 clearingPx,
//             bytes32 keccak256(abi.encode(Fill[])),
//             address settlement, uint256 chainId))
//   signature = personal_sign(inner)   // wallet.signMessage(arrayify(inner))
//   Fill = tuple(address trader, bool isBuy, uint256 quantity)
//
// js_params:
//   pkpId, encryptedDbUrl, epoch, pair, settlement, chainId, maxBatch

async function main({ pkpId, encryptedDbUrl, epoch, pair, settlement, chainId, maxBatch }) {
  const dbUrl = await Lit.Actions.Decrypt({ pkpId, ciphertext: encryptedDbUrl });

  const cap = Number(maxBatch) > 0 ? Number(maxBatch) : 200;
  const rows = (
    await neonQuery(
      dbUrl,
      "select id, ciphertext from orders where epoch = $1 and pair = $2 and not settled order by id asc limit $3",
      [epoch, pair, cap]
    )
  ).rows;

  if (rows.length > cap) {
    // Defensive: never silently match a partial book.
    throw new Error(`epoch ${epoch} has more than maxBatch (${cap}) orders`);
  }

  const orders = [];
  for (const row of rows) {
    const plain = await Lit.Actions.Decrypt({ pkpId, ciphertext: row.ciphertext });
    const o = JSON.parse(plain);
    orders.push({
      id: Number(row.id),
      side: o.side,
      limitPrice: BigInt(o.limitPrice),
      quantity: BigInt(o.quantity),
      trader: o.trader,
    });
  }

  const { clearingPx, fills } = runAuction(orders);

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
    orderIds: orders.map((o) => o.id), // every order in the batch; runEpoch marks them settled
    matchedOrders: orders.length,
    signer: wallet.address,
    signature,
  };
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

  // Candidate clearing prices = the distinct limit prices in the book.
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
    // Maximise matched volume; tie-break: smaller |demand-supply|, then lower price.
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

  // The short side fills fully (its total == V). The long side is rationed
  // pro-rata to sum to exactly V.
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

// Distribute `target` units across `side` proportional to each order's
// quantity, using floor + largest-remainder so the allocations sum to EXACTLY
// `target` (deterministic: remainder desc, then order id asc).
function ration(side, total, target) {
  if (target === total) return side.map((o) => ({ o, q: o.quantity }));
  const allocs = side.map((o) => {
    const num = o.quantity * target;
    return { o, q: num / total, rem: num % total };
  });
  let assigned = allocs.reduce((acc, a) => acc + a.q, 0n);
  let leftover = target - assigned; // < allocs.length
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

// Minimal Neon "SQL over HTTP" client (see encryptOrder.js for notes).
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
  const body = await res.json();
  if (!res.ok) {
    throw new Error(`neon sql ${res.status}: ${body && (body.message || JSON.stringify(body))}`);
  }
  const names = (body.fields || []).map((f) => f.name);
  return {
    rows: (body.rows || []).map((r) => Object.fromEntries(names.map((n, i) => [n, r[i]]))),
    rowCount: body.rowCount,
  };
}
