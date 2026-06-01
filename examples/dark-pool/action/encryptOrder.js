// Lit Action: seal a single dark-pool order and store it as ciphertext.
//
// The order's side / price / quantity never leave the TEE in the clear and
// never land in the database in plaintext. This action:
//   1. decrypts the DB connection string (itself an encrypted secret),
//   2. encrypts the order JSON (including the trader's signature) against the
//      vault PKP,
//   3. INSERTs (epoch, pair, ciphertext) into Postgres via Neon's SQL-over-HTTP.
//
// It returns only { ok, id } — never the order, never the connection string.
// Provider error bodies are not echoed (they could carry request data).
//
// The order carries a trader EIP-191 signature and a nonce. This action does a
// cheap shape check; matchEpoch is the authority that verifies the signature
// recovers to `trader` before the order can be matched. Storing an order here
// does NOT make it matchable — only a validly signed order survives matchEpoch.
//
// UNITS: `limitPrice` and `quantity` are decimal integer STRINGS scaled to chain
// units (quantity in base smallest-units; limitPrice in quote-per-base x1e18),
// each < 2^128. See submitOrder.js.
//
// js_params:
//   pkpId           vault PKP wallet address
//   encryptedDbUrl  ciphertext of the Neon connection string
//   epoch           batch id this order joins (integer)
//   pair            trading pair, e.g. "BASE/QUOTE"
//   order           { side, limitPrice, quantity, trader, nonce, sig }

const MAX_UINT128 = 2n ** 128n;

async function main({ pkpId, encryptedDbUrl, epoch, pair, order }) {
  if (!order || (order.side !== "buy" && order.side !== "sell")) {
    return { ok: false, reason: "order.side must be 'buy' or 'sell'" };
  }
  if (!inRange(order.limitPrice) || !inRange(order.quantity)) {
    return { ok: false, reason: "limitPrice and quantity must be positive integer strings < 2^128" };
  }
  if (typeof order.trader !== "string" || !/^0x[0-9a-fA-F]{40}$/.test(order.trader)) {
    return { ok: false, reason: "order.trader must be a 20-byte hex address" };
  }
  if (!/^[0-9]+$/.test(String(order.nonce || "")) || typeof order.sig !== "string" || !/^0x[0-9a-fA-F]+$/.test(order.sig)) {
    return { ok: false, reason: "order.nonce (uint string) and order.sig (hex) are required" };
  }
  if (!Number.isInteger(epoch) || epoch < 0) {
    return { ok: false, reason: "epoch must be a non-negative integer" };
  }

  const dbUrl = await Lit.Actions.Decrypt({ pkpId, ciphertext: encryptedDbUrl });

  const ciphertext = await Lit.Actions.Encrypt({
    pkpId,
    message: JSON.stringify({
      side: order.side,
      limitPrice: String(order.limitPrice),
      quantity: String(order.quantity),
      trader: order.trader,
      nonce: String(order.nonce),
      sig: order.sig,
    }),
  });

  const res = await neonQuery(
    dbUrl,
    "insert into orders (epoch, pair, ciphertext) values ($1, $2, $3) returning id",
    [epoch, pair, ciphertext]
  );

  return { ok: true, id: res.rows[0] && res.rows[0].id };
}

function inRange(v) {
  if (typeof v !== "string" || !/^[0-9]+$/.test(v)) return false;
  let n;
  try {
    n = BigInt(v);
  } catch {
    return false;
  }
  return n > 0n && n < MAX_UINT128;
}

// Minimal Neon "SQL over HTTP" client (see matchEpoch.js for notes). Error
// bodies are not surfaced — only the status code.
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
