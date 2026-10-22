// Thin client for the Blockchair public REST API (Zcash mainnet): list a
// transparent address's UTXOs, read the chain tip, and broadcast a raw tx.
//
// Why Blockchair: Zcash *testnet* REST infrastructure is effectively dead
// (the old explorer.testnet.z.cash Insight API is gone, there is no public
// testnet Blockbook, and Blockchair has no testnet), so this example targets
// MAINNET, where Blockchair's REST API is the dependable public option.
//
// The free tier is rate-limited; if you hit limits, set BLOCKCHAIR_API_KEY in
// .env and it'll be appended to every request.

const BASE = "https://api.blockchair.com/zcash";
const ZAT_PER_ZEC = 100_000_000;

function withKey(url) {
  const key = process.env.BLOCKCHAIR_API_KEY;
  if (!key) return url;
  return url + (url.includes("?") ? "&" : "?") + `key=${key}`;
}

async function getJson(url) {
  const res = await fetch(withKey(url));
  const body = await res.json().catch(() => ({}));
  if (!res.ok) {
    const msg = body && body.context && body.context.error;
    throw new Error(`${url} -> ${res.status}${msg ? `: ${msg}` : ""}`);
  }
  return body;
}

// Unspent outputs for a transparent address. Each entry: { txid, vout, value }
// where value is an integer string in zatoshi (matching the action's interface).
async function getUtxos(address) {
  const body = await getJson(`${BASE}/dashboards/address/${address}?limit=100`);
  const entry = body.data && body.data[address];
  if (!entry) throw new Error(`no dashboard data for ${address}`);
  return (entry.utxo || []).map((u) => ({
    txid: u.transaction_hash,
    vout: u.index,
    value: String(u.value), // Blockchair returns Zcash values in zatoshi
  }));
}

// Confirmed balance (zatoshi, as a number) for an address.
async function getBalance(address) {
  const body = await getJson(`${BASE}/dashboards/address/${address}`);
  const entry = body.data && body.data[address];
  return entry && entry.address ? Number(entry.address.balance || 0) : 0;
}

// Current best block height — used to set a sane nExpiryHeight.
async function getTipHeight() {
  const body = await getJson(`${BASE}/stats`);
  const h = body.data && (body.data.best_block_height ?? body.data.blocks);
  if (!h) throw new Error("could not read tip height from /stats");
  return Number(h);
}

// Broadcast a raw (hex) transaction. Returns the txid on success.
async function broadcast(txHex) {
  const res = await fetch(withKey(`${BASE}/push/transaction`), {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ data: txHex }).toString(),
  });
  const body = await res.json().catch(() => ({}));
  if (!res.ok || !body.data || !body.data.transaction_hash) {
    const msg = (body.context && body.context.error) || JSON.stringify(body);
    throw new Error(`broadcast failed (${res.status}): ${msg}`);
  }
  return body.data.transaction_hash;
}

const zatToZec = (zat) => Number(zat) / ZAT_PER_ZEC;
const zecToZat = (zec) => Math.round(Number(zec) * ZAT_PER_ZEC);

module.exports = {
  getUtxos,
  getBalance,
  getTipHeight,
  broadcast,
  zatToZec,
  zecToZat,
  ZAT_PER_ZEC,
};
