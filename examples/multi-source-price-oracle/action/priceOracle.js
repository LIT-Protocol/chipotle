// Lit Action: fetch the spot price for an asset from three independent
// public price sources (Coinbase, Kraken, Bitstamp), take the median, and
// sign an attestation the PriceOracle contract can verify.
//
// Median-of-three is the right aggregation for live market prices:
//   * Different venues disagree by a few cents at any moment, so the strict
//     byte-equality used by the multi-RPC consensus example would never
//     pass.
//   * Median naturally rejects one outlier — a single exchange returning a
//     stale, frozen, or manipulated price doesn't shift the result.
//   * We also check the spread between the lowest and highest reported
//     prices and refuse to sign if it exceeds `maxSpreadBps` (default 1%).
//     A 5% gap between Coinbase and Kraken is a sign something is wrong —
//     halt and let a human investigate.
//
// All three sources are public, keyless HTTP endpoints — no API keys, no
// PKP, no encryption required. The signature is produced by
// Lit.Actions.getLitActionPrivateKey(), an identity derived from this
// action's IPFS CID, so the contract trusts THIS exact aggregation logic.
//
// js_params:
//   asset              Symbol — currently "ETH", "BTC", or "SOL"
//   registryAddress    Address of the PriceOracle contract
//   registryChainId    Chain id where the registry lives
//   deadline           Signature expiry (unix seconds)
//
// The safety thresholds (max spread, minimum sources, decimals) are
// HARDCODED below rather than taken from js_params. Caller-supplied
// safety knobs are theatre: anyone with the usage key could otherwise
// request a signed reading with `minSources: 1, maxSpreadBps: 99999`
// and bypass the median-of-three / 1% spread story the README
// promises. To change a threshold, edit the constants — that mints a
// new action CID, which changes the signer address, which forces a
// redeploy of PriceOracle. That's the trust model: thresholds are
// content-addressed, not configurable per call.

// Per-source symbol mappings. Kraken's legacy ticker has BTC=XBT and
// returns result keys with X/Z prefixes (XXBTZUSD, XETHZUSD); the others
// are predictable.
const SYMBOLS = {
  ETH: { coinbase: "ETH-USD", kraken: "ETHUSD", krakenKey: "XETHZUSD", bitstamp: "ethusd" },
  BTC: { coinbase: "BTC-USD", kraken: "XBTUSD", krakenKey: "XXBTZUSD", bitstamp: "btcusd" },
  SOL: { coinbase: "SOL-USD", kraken: "SOLUSD", krakenKey: "SOLUSD",   bitstamp: "solusd" },
};

// Safety thresholds — see "trust model" comment at the top.
// Edit these constants to tighten / loosen, but doing so changes the
// action's IPFS CID and therefore its signer address, requiring a
// redeploy of the PriceOracle contract.
const MAX_SPREAD_BPS = 100; // (max - min) / median, in basis points
const MIN_SOURCES = 2; // require at least this many successful fetches
const DECIMALS = 8; // fixed-point precision for the signed price

const SOURCES = [
  {
    name: "coinbase",
    fetch: async (symbols) => {
      const res = await fetch(`https://api.coinbase.com/v2/prices/${symbols.coinbase}/spot`);
      if (!res.ok) throw new Error(`coinbase ${res.status}`);
      const body = await res.json();
      return Number(body.data.amount);
    },
  },
  {
    name: "kraken",
    fetch: async (symbols) => {
      const res = await fetch(`https://api.kraken.com/0/public/Ticker?pair=${symbols.kraken}`);
      if (!res.ok) throw new Error(`kraken ${res.status}`);
      const body = await res.json();
      if (body.error && body.error.length) throw new Error(`kraken ${body.error.join(",")}`);
      // Kraken returns the result keyed by its canonical pair name (which
      // for legacy markets has X/Z prefixes). Take the first (and only) entry.
      const entry = body.result[symbols.krakenKey] || Object.values(body.result)[0];
      return Number(entry.c[0]); // c = last-trade-closed [price, lot volume]
    },
  },
  {
    name: "bitstamp",
    fetch: async (symbols) => {
      const res = await fetch(`https://www.bitstamp.net/api/v2/ticker/${symbols.bitstamp}/`);
      if (!res.ok) throw new Error(`bitstamp ${res.status}`);
      const body = await res.json();
      return Number(body.last);
    },
  },
];

async function main({
  asset,
  registryAddress,
  registryChainId,
  deadline,
}) {
  const symbols = SYMBOLS[asset];
  if (!symbols) {
    return {
      authorized: false,
      reason: `unsupported asset: ${asset}. Supported: ${Object.keys(SYMBOLS).join(", ")}`,
    };
  }

  // Fire all three sources in parallel.
  const settled = await Promise.allSettled(
    SOURCES.map(async (src) => ({ name: src.name, price: await src.fetch(symbols) }))
  );

  const successful = [];
  const failed = [];
  settled.forEach((r, i) => {
    if (r.status === "fulfilled" && Number.isFinite(r.value.price) && r.value.price > 0) {
      successful.push(r.value);
    } else {
      failed.push({
        name: SOURCES[i].name,
        error: r.status === "rejected" ? r.reason.message : "invalid price",
      });
    }
  });

  if (successful.length < MIN_SOURCES) {
    return {
      authorized: false,
      reason: `only ${successful.length}/${SOURCES.length} sources succeeded (need ${MIN_SOURCES})`,
      failed,
    };
  }

  const prices = successful.map((s) => s.price).sort((a, b) => a - b);
  const median =
    prices.length % 2 === 1
      ? prices[(prices.length - 1) / 2]
      : (prices[prices.length / 2 - 1] + prices[prices.length / 2]) / 2;
  const spreadBps = Math.round(((prices[prices.length - 1] - prices[0]) / median) * 10000);

  if (spreadBps > MAX_SPREAD_BPS) {
    return {
      authorized: false,
      reason: `spread ${spreadBps} bps exceeds max ${MAX_SPREAD_BPS} bps`,
      sources: successful,
      spreadBps,
    };
  }

  // Convert the median to fixed-point. Use string concatenation +
  // BigInt rather than `median * 10**DECIMALS` so we don't lose
  // precision (or overflow Number.MAX_SAFE_INTEGER) if DECIMALS gets
  // bumped to 18.
  const priceInt = scaleToFixedPoint(median, DECIMALS);

  const observedAt = Math.floor(Date.now() / 1000);

  // Must match `keccak256(abi.encode(...))` in the contract.
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["string", "uint256", "uint8", "uint256", "uint256", "address", "uint256"],
      [asset, priceInt, DECIMALS, observedAt, deadline, registryAddress, registryChainId]
    )
  );

  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const signature = await wallet.signMessage(ethers.utils.arrayify(digest));

  return {
    authorized: true,
    signature,
    signer: wallet.address,
    asset,
    price: priceInt.toString(),
    priceFloat: median,
    decimals: DECIMALS,
    observedAt,
    spreadBps,
    sources: successful,
    failed,
  };
}

// Scale a JS Number (e.g. 2104.62) to an integer with `decimals`
// fractional digits, returned as a BigInt. Safe for decimals=18:
// captures Number's available precision via toFixed(min(decimals,8))
// then zero-pads the remainder in BigInt land.
function scaleToFixedPoint(value, decimals) {
  const SAFE_FRACTIONAL = 8;
  const fracDigits = Math.min(decimals, SAFE_FRACTIONAL);
  const padDigits = decimals - fracDigits;
  const fixed = value.toFixed(fracDigits);
  const negative = fixed.startsWith("-");
  const [whole, frac = ""] = (negative ? fixed.slice(1) : fixed).split(".");
  const padded = frac.padEnd(fracDigits, "0") + "0".repeat(padDigits);
  const magnitude = BigInt(whole + padded);
  return negative ? -magnitude : magnitude;
}
