// lit-action-oracles.js
//
// Consensus & oracles. Strict byte-for-byte agreement for discrete facts, median
// + spread check for continuous values, a signed price feed, and categorical
// LLM consensus. All deny (fail closed) when agreement is not reached.

import { deny, keccak256Utf8 } from "./lit-action-core.js";
import { signDigest } from "./lit-action-signing.js";

function requireNonEmptyArray(name, value) {
  if (!Array.isArray(value) || value.length === 0) {
    deny(`${name} must be a non-empty array`);
  }
  return value;
}

function requirePositiveInteger(name, value, max) {
  const n = Number(value);
  if (!Number.isInteger(n) || n < 1) {
    deny(`${name} must be an integer >= 1`);
  }
  if (max !== undefined && n > max) {
    deny(`${name} ${n} exceeds available count ${max}`);
  }
  return n;
}

function requireUint8(name, value) {
  const n = Number(value);
  if (!Number.isInteger(n) || n < 0 || n > 255) {
    deny(`${name} must be an integer between 0 and 255`);
  }
  return n;
}

/**
 * For discrete facts (sanctioned y/n, event presence). Fetch each source via
 * `fetchSource(source)`, require at least `minSources` successes, and deny
 * unless they all agree byte-for-byte (deep-equal via JSON). Returns the agreed
 * value.
 */
export async function requireStrictAgreement({ sources, fetchSource, minSources }) {
  const src = requireNonEmptyArray("sources", sources);
  if (typeof fetchSource !== "function") deny("fetchSource must be a function");
  const min = requirePositiveInteger("minSources", minSources, src.length);
  const settled = await Promise.allSettled(src.map((s) => fetchSource(s)));
  const ok = settled.filter((r) => r.status === "fulfilled").map((r) => r.value);
  if (ok.length < min) {
    deny(`only ${ok.length} sources responded, need ${min}`);
  }
  const canonical = JSON.stringify(ok[0]);
  if (!ok.every((v) => JSON.stringify(v) === canonical)) {
    deny("sources disagree");
  }
  return ok[0];
}

/**
 * For continuous values. Take the median of `observations` (bigint fixed-point
 * recommended) and reject if (max-min)/median exceeds `maxSpreadBps`. Requires
 * at least `minSources` observations. Returns the median.
 */
export function medianWithSpreadCheck({ observations, minSources, maxSpreadBps }) {
  const raw = requireNonEmptyArray("observations", observations);
  const min = requirePositiveInteger("minSources", minSources, raw.length);
  let maxSpread;
  try {
    maxSpread = BigInt(maxSpreadBps);
  } catch {
    deny("maxSpreadBps must be an integer");
  }
  if (maxSpread < 0n) deny("maxSpreadBps must be non-negative");
  const obs = raw.map((o) => BigInt(o)).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  if (obs.length < min) {
    deny(`only ${obs.length} observations, need ${min}`);
  }
  const mid = obs.length >> 1;
  const median = obs.length % 2 ? obs[mid] : (obs[mid - 1] + obs[mid]) / 2n;
  if (median === 0n) deny("median is zero");
  const spreadBps = ((obs[obs.length - 1] - obs[0]) * 10000n) / median;
  if (spreadBps > maxSpread) {
    deny(`spread ${spreadBps}bps exceeds cap ${maxSpreadBps}bps`);
  }
  return median;
}

// Decimal string -> integer fixed-point with `decimals` places, using BigInt
// throughout. "65000.12", 18 -> 65000120000000000000000n
function toFixedPoint(decimalStr, decimals) {
  const [whole, frac = ""] = String(decimalStr).split(".");
  const fracPadded = (frac + "0".repeat(decimals)).slice(0, decimals);
  return BigInt(whole + fracPadded);
}

/**
 * Fetch N exchange prices, median-with-spread-check them, scale to fixed-point
 * via BigInt (NOT Math.round -- avoids float overflow at 18 decimals), and sign
 * the result with the action key. `sources` is an array of async fns returning
 * a decimal price string (e.g. "65000.12"). Returns JSON-safe price/median
 * strings plus { signature, signer, digest, assetId }.
 */
export async function signedPriceFeed({ asset, sources, decimals, registry, deadline }) {
  const src = requireNonEmptyArray("sources", sources);
  if (!src.every((fetchPrice) => typeof fetchPrice === "function")) {
    deny("sources must be functions");
  }
  const dec = requireUint8("decimals", decimals);
  const prices = await Promise.all(src.map((fetchPrice) => fetchPrice()));
  const scaled = prices.map((p) => toFixedPoint(p, dec));
  const median = medianWithSpreadCheck({
    observations: scaled,
    minSources: src.length,
    maxSpreadBps: 100n,
  });
  const assetId = keccak256Utf8(asset);
  const { signature, signer, digest } = await signDigest({
    types: ["address", "bytes32", "uint256", "uint8", "uint256"],
    values: [registry, assetId, median, dec, deadline],
    useAction: true,
  });
  return { price: median.toString(), median: median.toString(), signature, signer, digest, assetId };
}

/**
 * Strict agreement across LLM providers for a categorical YES/NO/UNCLEAR answer.
 * `providers` is an array of async fns returning one of those strings. Binds
 * questionId = keccak256(questionText) so the prompt can't be swapped under the
 * answer. Returns { questionId, answer, agreed } when >= minAgreement providers
 * give the same answer; otherwise denies.
 */
export async function aiConsensus({ question, providers, minAgreement }) {
  const prov = requireNonEmptyArray("providers", providers);
  if (!prov.every((p) => typeof p === "function")) deny("providers must be functions");
  const min = requirePositiveInteger("minAgreement", minAgreement, prov.length);
  const questionId = keccak256Utf8(question);
  const settled = await Promise.allSettled(prov.map((p) => p()));
  const votes = settled
    .filter((r) => r.status === "fulfilled")
    .map((r) => String(r.value).toUpperCase())
    .filter((v) => v === "YES" || v === "NO" || v === "UNCLEAR");
  const tally = votes.reduce((m, v) => ((m[v] = (m[v] || 0) + 1), m), {});
  const [answer, count] = Object.entries(tally).sort((a, b) => b[1] - a[1])[0] || [];
  if (!answer || count < min) {
    deny(`no ${min}-way agreement (tally ${JSON.stringify(tally)})`);
  }
  return { questionId, answer, agreed: count };
}
