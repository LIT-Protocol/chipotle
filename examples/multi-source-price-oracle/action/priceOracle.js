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
//   maxSpreadBps       Optional, default 100 (= 1%) — abort if (max-min)/median exceeds this
//   minSources         Optional, default 2 — require at least this many successful fetches
//   decimals           Optional, default 8 — fixed-point precision for the signed price

// Per-source symbol mappings. Kraken's legacy ticker has BTC=XBT and
// returns result keys with X/Z prefixes (XXBTZUSD, XETHZUSD); the others
// are predictable.
const SYMBOLS = {
  ETH: { coinbase: "ETH-USD", kraken: "ETHUSD", krakenKey: "XETHZUSD", bitstamp: "ethusd" },
  BTC: { coinbase: "BTC-USD", kraken: "XBTUSD", krakenKey: "XXBTZUSD", bitstamp: "btcusd" },
  SOL: { coinbase: "SOL-USD", kraken: "SOLUSD", krakenKey: "SOLUSD",   bitstamp: "solusd" },
};

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
  maxSpreadBps = 100,
  minSources = 2,
  decimals = 8,
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

  if (successful.length < minSources) {
    return {
      authorized: false,
      reason: `only ${successful.length}/${SOURCES.length} sources succeeded (need ${minSources})`,
      failed,
    };
  }

  const prices = successful.map((s) => s.price).sort((a, b) => a - b);
  const median =
    prices.length % 2 === 1
      ? prices[(prices.length - 1) / 2]
      : (prices[prices.length / 2 - 1] + prices[prices.length / 2]) / 2;
  const spreadBps = Math.round(((prices[prices.length - 1] - prices[0]) / median) * 10000);

  if (spreadBps > maxSpreadBps) {
    return {
      authorized: false,
      reason: `spread ${spreadBps} bps exceeds max ${maxSpreadBps} bps`,
      sources: successful,
      spreadBps,
    };
  }

  // Convert the median to fixed-point. Round half-up.
  const scale = 10 ** decimals;
  const priceInt = BigInt(Math.round(median * scale));

  const observedAt = Math.floor(Date.now() / 1000);

  // Must match `keccak256(abi.encode(...))` in the contract.
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["string", "uint256", "uint8", "uint256", "uint256", "address", "uint256"],
      [asset, priceInt, decimals, observedAt, deadline, registryAddress, registryChainId]
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
    decimals,
    observedAt,
    spreadBps,
    sources: successful,
    failed,
  };
}
