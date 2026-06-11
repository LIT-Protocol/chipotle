// Build the snapshot code blob (lit-venues IIFE bundle + the action source)
// and run code against /lit_action with the scoped usage key, unwrapping the
// response envelope:
//   { response: <whatever main returned>, logs: "...", has_error: bool }
//
// The SAME concatenation is used by setup.js (to compute the pinned CID) and
// by snapshot.js (to execute). Byte-identical code is what makes the CID
// pinning — and therefore the Decrypt grant on the sealed credentials — hold.

const fs = require("fs");
const path = require("path");

const ACTION_FILE = path.join(__dirname, "..", "action", "portfolio-snapshot.js");
const BUNDLE_FILE = path.join(
  __dirname, "..", "..", "..", "lit-venues", "dist", "lit-venues.iife.js"
);

function buildCode() {
  if (!fs.existsSync(BUNDLE_FILE)) {
    throw new Error(
      `lit-venues bundle not found at ${BUNDLE_FILE}\n` +
        "Build it first:\n" +
        "  cd ../../lit-venues && npm install && npm run build"
    );
  }
  const bundle = fs.readFileSync(BUNDLE_FILE, "utf8");
  const main = fs.readFileSync(ACTION_FILE, "utf8");
  return `${bundle}\n${main}`;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function runCodeOnce(code, jsParams) {
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

// Run code, retrying on failure. A freshly-minted usage key's
// execute-in-group grant is eventually consistent, so the *first* call right
// after `add_usage_api_key` can fail for a beat while the grant propagates.
// setup.js calls this with retries on its first run; steady-state callers
// (snapshot.js) leave retries at 0.
async function runCode(code, jsParams, { retries = 0, delayMs = 2500 } = {}) {
  let lastErr;
  for (let attempt = 0; attempt <= retries; attempt++) {
    try {
      return await runCodeOnce(code, jsParams);
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

module.exports = { buildCode, runCode, ACTION_FILE, BUNDLE_FILE };
