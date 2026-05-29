// Lit Action: seal a single dark-pool order and store it as ciphertext.
//
// The order's side / price / quantity never leave the TEE in the clear and
// never land in the database in plaintext. This action:
//   1. decrypts the DB connection string (itself an encrypted secret),
//   2. encrypts the order JSON against the vault PKP,
//   3. INSERTs (epoch, pair, ciphertext) into Postgres via Neon's SQL-over-HTTP.
//
// It returns only { ok, id } — never the order, never the connection string.
// (Returning or logging either would leak it: action return values and logs are
// visible to the caller.)
//
// PRICE/QTY UNITS: the caller passes `limitPrice` and `quantity` as decimal
// integer STRINGS already scaled to chain units — quantity in base
// smallest-units, limitPrice in quote-units-per-base-unit times 1e18. matchEpoch
// and DarkPoolSettlement.sol use those same units. See submitOrder.js.
//
// js_params:
//   pkpId           vault PKP wallet address (encrypts orders AND the db url)
//   encryptedDbUrl  ciphertext of the Neon connection string
//   epoch           batch id this order joins (integer)
//   pair            trading pair, e.g. "BASE/QUOTE"
//   order           { side: "buy"|"sell", limitPrice, quantity, trader }

async function main({ pkpId, encryptedDbUrl, epoch, pair, order }) {
  // --- shape validation (cheap, before we touch crypto or the network) ---
  if (!order || (order.side !== "buy" && order.side !== "sell")) {
    return { ok: false, reason: "order.side must be 'buy' or 'sell'" };
  }
  if (!isPositiveIntString(order.limitPrice) || !isPositiveIntString(order.quantity)) {
    return { ok: false, reason: "limitPrice and quantity must be positive integer strings" };
  }
  if (!/^0x[0-9a-fA-F]{40}$/.test(order.trader || "")) {
    return { ok: false, reason: "order.trader must be a 20-byte hex address" };
  }
  if (!Number.isInteger(epoch) || epoch < 0) {
    return { ok: false, reason: "epoch must be a non-negative integer" };
  }

  // --- decrypt the DB credential inside the enclave ---
  const dbUrl = await Lit.Actions.Decrypt({ pkpId, ciphertext: encryptedDbUrl });

  // --- seal the order ---
  const ciphertext = await Lit.Actions.Encrypt({
    pkpId,
    message: JSON.stringify({
      side: order.side,
      limitPrice: String(order.limitPrice),
      quantity: String(order.quantity),
      trader: order.trader,
    }),
  });

  // --- store ciphertext + routing metadata; nothing in plaintext ---
  const res = await neonQuery(
    dbUrl,
    "insert into orders (epoch, pair, ciphertext) values ($1, $2, $3) returning id",
    [epoch, pair, ciphertext]
  );

  return { ok: true, id: res.rows[0] && res.rows[0].id };
}

function isPositiveIntString(v) {
  return typeof v === "string" && /^[0-9]+$/.test(v) && v !== "0" && /[1-9]/.test(v);
}

// Minimal Neon "SQL over HTTP" client. The endpoint is derived from the
// connection string host; the full connection string travels in the
// Neon-Connection-String header. Array mode returns rows as arrays, which we
// re-key into objects using the returned field names.
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
