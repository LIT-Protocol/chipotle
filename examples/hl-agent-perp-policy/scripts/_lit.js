// Run the perpPolicy action against the Lit API with the scoped usage key,
// and unwrap the /lit_action envelope into the action's own return value.
//
// /lit_action wraps the action's return as
//   { response: <whatever you returned>, logs: "...", has_error: bool }
//
// The action uses the lit-venues connectors, so the submitted code is the
// prebuilt lit-venues IIFE bundle (global `LitVenues`) concatenated above the
// action source — the pattern from e2e/tests/api/lit-venues-spike.spec.ts.
// buildCode() is the single source of truth for that concatenation: setup.js
// computes the CID from it and runAction() submits it. This matters more here
// than anywhere else: the agent key is derived from the CID of EXACTLY these
// bytes, so the bundle on disk is part of the wallet's identity.

const fs = require("fs");
const path = require("path");

const ACTION_FILE = path.join(__dirname, "..", "action", "perpPolicy.js");
const BUNDLE_PATH = path.join(
  __dirname,
  "..",
  "..",
  "..",
  "lit-venues",
  "dist",
  "lit-venues.iife.js"
);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function buildCode() {
  if (!fs.existsSync(BUNDLE_PATH)) {
    throw new Error(
      `lit-venues bundle missing at ${BUNDLE_PATH} — run \`npm install && npm run build\` in lit-venues/`
    );
  }
  const bundle = fs.readFileSync(BUNDLE_PATH, "utf8");
  const main = fs.readFileSync(ACTION_FILE, "utf8");
  return `${bundle}\n${main}`;
}

async function runActionOnce(jsParams) {
  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    LIT_USAGE_API_KEY,
  } = process.env;
  if (!LIT_USAGE_API_KEY) {
    throw new Error("LIT_USAGE_API_KEY is required (run `npm run setup` first)");
  }

  const code = buildCode();
  const res = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
    method: "POST",
    headers: {
      "X-Api-Key": LIT_USAGE_API_KEY,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ code, js_params: jsParams }),
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

// Run the action, retrying on failure. A freshly-minted usage key's
// execute-in-group grant is eventually consistent, so the *first* call right
// after `add_usage_api_key` can fail for a beat while the grant propagates —
// setup.js calls the read-only "address" branch with retries. Steady-state
// callers (approve-agent / trade) leave retries at 0: blindly retrying a
// trade could double-place an order.
async function runAction(jsParams, { retries = 0, delayMs = 2500 } = {}) {
  let lastErr;
  for (let attempt = 0; attempt <= retries; attempt++) {
    try {
      return await runActionOnce(jsParams);
    } catch (err) {
      lastErr = err;
      if (attempt < retries) {
        console.log(
          `  ...action not ready yet (attempt ${attempt + 1}/${retries + 1}), retrying in ${delayMs}ms`
        );
        await sleep(delayMs);
      }
    }
  }
  throw lastErr;
}

module.exports = { runAction, buildCode };
