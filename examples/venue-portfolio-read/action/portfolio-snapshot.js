// Lit Action: attested multi-venue balance snapshot (read scope).
//
// The lit-venues IIFE bundle is concatenated ABOVE this file by
// scripts/_lit.js (global `LitVenues`); the pinned CID covers bundle + this
// source together. The flow:
//   1. decrypt the venue-credentials-v1 blob — possible only inside the TEE,
//      and only for this exact code (the group pins this CID),
//   2. fetch balances from each configured venue (binance / coinbase /
//      hyperliquid, each optional — hyperliquid reads need only an account
//      address, no key),
//   3. merge per-venue balances and sum totals per asset with exact decimal
//      math (LitVenues.addDec — no float drift).
//
// One venue failing (geo-block, bad key, venue down) does not kill the
// snapshot: the error lands in that venue's slot with the unified
// lit-venues error taxonomy and the rest still report.
//
// Outbound fetches: exactly one per configured venue (max 3) — far inside
// the runtime's 50-fetch quota and this repo's <=10-per-action discipline.
// Decrypt is a TEE op, not a fetch.
//
// js_params:
//   pkpId        vault PKP wallet address the credentials are sealed against
//   sealedCreds  ciphertext of the venue-credentials-v1 JSON:
//                {
//                  v: 1,
//                  egress?:      { proxyUrl },                        // plan D4
//                  binance?:     { apiKey, secret, keyType, sandbox },
//                  coinbase?:    { apiKey, secret, keyType },         // es256-jwt
//                  hyperliquid?: { accountAddress, sandbox },         // no key
//                }
//
// Returns { ok, ts, venues: { <venueId>: { ok, balances | error } }, totals }.

const VENUE_IDS = ["binance", "coinbase", "hyperliquid"];

async function main({ pkpId, sealedCreds }) {
  if (!pkpId || !sealedCreds) {
    return { ok: false, reason: "js_params pkpId and sealedCreds are required" };
  }

  // venue-credentials-v1: plaintext exists only here, inside the enclave.
  let config;
  try {
    config = JSON.parse(await Lit.Actions.Decrypt({ pkpId, ciphertext: sealedCreds }));
  } catch (e) {
    // Never echo decrypt internals — a wrong pkpId/ciphertext pair is all this can be.
    return { ok: false, reason: "could not decrypt or parse the sealed credentials" };
  }

  const proxy = (config.egress && config.egress.proxyUrl) || undefined;
  const venues = {};
  const totals = {};
  let okCount = 0;

  for (const venueId of VENUE_IDS) {
    const entry = config[venueId];
    if (!entry) continue; // every venue is optional

    try {
      const client = LitVenues.createVenue(venueConfig(venueId, entry, proxy));
      const balances = await client.fetchBalances(); // 1 outbound fetch
      venues[venueId] = { ok: true, balances };
      okCount += 1;
      for (const b of balances) {
        // Exact decimal sums across venues, keyed by asset symbol.
        totals[b.asset] = totals[b.asset] ? LitVenues.addDec(totals[b.asset], b.total) : b.total;
      }
    } catch (e) {
      venues[venueId] = {
        ok: false,
        error: {
          code: (e && e.code) || "unknown", // lit-venues taxonomy: auth | rate_limited | venue_unavailable | ...
          message: String((e && e.message) || e).slice(0, 200),
        },
      };
    }
  }

  if (Object.keys(venues).length === 0) {
    return { ok: false, reason: "sealed credentials configure no venues" };
  }
  return { ok: okCount > 0, ts: Date.now(), venues, totals };
}

function venueConfig(venueId, entry, proxy) {
  if (venueId === "hyperliquid") {
    // Reads need only the account address whose state to report — no key.
    return {
      venueId,
      sandbox: entry.sandbox === true,
      proxy,
      credentials: { keyType: "pkp-eip712", accountAddress: entry.accountAddress },
    };
  }
  // CEX venues: apiKey + secret. keyType: hmac | ed25519 (binance),
  // es256-jwt (coinbase). coinbase has no sandbox (createVenue would throw).
  return {
    venueId,
    sandbox: entry.sandbox === true,
    proxy,
    credentials: { apiKey: entry.apiKey, secret: entry.secret, keyType: entry.keyType },
  };
}
