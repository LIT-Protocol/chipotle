// Run the zcashSigner action against the Lit API with the scoped usage key,
// and unwrap the /lit_action envelope into the action's own return value.
//
// /lit_action wraps the action's return as
//   { response: <whatever you returned>, logs: "...", has_error: bool }

const fs = require("fs");
const path = require("path");

const ACTION_FILE = path.join(__dirname, "..", "action", "zcashSigner.js");

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function runActionOnce(jsParams) {
  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    LIT_USAGE_API_KEY,
  } = process.env;
  if (!LIT_USAGE_API_KEY) {
    throw new Error("LIT_USAGE_API_KEY is required (run `npm run setup` first)");
  }

  const code = fs.readFileSync(ACTION_FILE, "utf8");
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
// after `add_usage_api_key` can fail for a beat while the grant propagates.
// The docs say not to sleep a fixed amount but to poll the real execution path
// until it succeeds — so `setup.js` calls this with retries on its first run.
// Steady-state callers (address/balance/transfer) leave retries at 0.
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

module.exports = { runAction, ACTION_FILE };
