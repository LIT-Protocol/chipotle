// Compose the lit-venues bundle + the stop action into one executable, run it
// against /lit_action with the scoped usage key, and unwrap the response
// envelope into the action's own return value.
//
// The lit-venues IIFE bundle (global `LitVenues`) is concatenated ABOVE the
// action source — the same inline-bundle pattern proven by
// e2e/tests/api/lit-venues-spike.spec.ts. Build the bundle first:
//   cd ../../lit-venues && npm install && npm run build
//
// /lit_action wraps the action's return as
//   { response: <whatever you returned>, logs: "...", has_error: bool }

const fs = require("fs");
const path = require("path");

const ACTION_FILE = path.join(__dirname, "..", "action", "priceStop.js");
const BUNDLE_FILE = path.join(__dirname, "..", "..", "..", "lit-venues", "dist", "lit-venues.iife.js");

function composeCode() {
  if (!fs.existsSync(BUNDLE_FILE)) {
    throw new Error(
      `lit-venues bundle missing at ${BUNDLE_FILE}\n` +
        "Build it first: cd ../../lit-venues && npm install && npm run build"
    );
  }
  const bundle = fs.readFileSync(BUNDLE_FILE, "utf8");
  const action = fs.readFileSync(ACTION_FILE, "utf8");
  return `${bundle}\n${action}`;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function runActionOnce(jsParams) {
  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    LIT_USAGE_API_KEY,
  } = process.env;
  if (!LIT_USAGE_API_KEY) {
    throw new Error("LIT_USAGE_API_KEY is required (run `npm run setup` first)");
  }

  const res = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
    method: "POST",
    headers: {
      "X-Api-Key": LIT_USAGE_API_KEY,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ code: composeCode(), js_params: jsParams }),
  });
  const envelope = await res.json();
  if (!res.ok) {
    throw new Error(`lit_action -> ${res.status}: ${JSON.stringify(envelope)}`);
  }
  if (envelope.has_error) {
    throw new Error(`action errored: ${envelope.logs || JSON.stringify(envelope)}`);
  }
  return envelope.response;
}

// A freshly-minted usage key's execute-in-group grant is eventually
// consistent, so the FIRST call (setup's probe) polls until it propagates.
//
// Retries are deliberately gated by `retryOn`, default: ONLY auth/permission
// errors. A triggered stop must never be blind-retried — the failed attempt
// may already have placed the sell venue-side.
const isGrantPropagation = (err) => /->\s*(401|403)/.test(String(err && err.message));

async function runAction(jsParams, { retries = 0, delayMs = 3000, retryOn = isGrantPropagation } = {}) {
  let lastErr;
  for (let attempt = 0; attempt <= retries; attempt++) {
    try {
      return await runActionOnce(jsParams);
    } catch (err) {
      lastErr = err;
      if (attempt < retries && retryOn(err)) {
        console.log(
          `  ...action not ready yet (attempt ${attempt + 1}/${retries + 1}), retrying in ${delayMs}ms`
        );
        await sleep(delayMs);
      } else {
        throw err;
      }
    }
  }
  throw lastErr;
}

module.exports = { runAction, composeCode, ACTION_FILE, BUNDLE_FILE };
