// Shared helper: run a specific user's action-bound-wallet Lit Action.
//
// Each user has their own action source (their address stamped into the
// template), so this builds that source on the fly and executes it with the
// scoped usage key. Returns the unwrapped action response.

const { actionSourceFor } = require("./_users");

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

// Retry transient failures (network errors, 5xx) with linear backoff. Permanent
// failures (4xx, action has_error) surface immediately — retrying them just
// wastes time. This is the "exercise the real path, retry till it works"
// instinct scoped to one call; waitForUsageKeyReady below applies it to setup.
async function withRetry(fn, { attempts = 4, baseDelayMs = 1500, label = "request" } = {}) {
  let lastErr;
  for (let i = 1; i <= attempts; i++) {
    try {
      return await fn();
    } catch (err) {
      lastErr = err;
      if (!err.transient || i === attempts) throw err;
      const delay = baseDelayMs * i;
      console.warn(`  ${label} failed (${err.message}); retrying in ${delay}ms [${i}/${attempts - 1}]`);
      await wait(delay);
    }
  }
  throw lastErr;
}

async function postLitAction(code, jsParams) {
  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    LIT_USAGE_API_KEY,
  } = process.env;

  if (!LIT_USAGE_API_KEY) {
    throw new Error("LIT_USAGE_API_KEY is required (run `npm run setup`)");
  }

  let res;
  try {
    res = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
      method: "POST",
      headers: {
        "X-Api-Key": LIT_USAGE_API_KEY,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ code, js_params: jsParams }),
    });
  } catch (err) {
    err.transient = true; // network-level failure (node's "fetch failed")
    throw err;
  }

  if (res.status >= 500) {
    const body = await res.text();
    const err = new Error(`lit_action -> ${res.status}: ${body.slice(0, 160)}`);
    err.transient = true;
    throw err;
  }

  // /lit_action wraps the action's return as { response, logs, has_error }.
  const envelope = await res.json();
  if (envelope.has_error) {
    throw new Error(`Lit Action errored: ${envelope.logs || JSON.stringify(envelope)}`);
  }
  return envelope.response;
}

async function runUserAction(ownerAddress, jsParams) {
  const code = actionSourceFor(ownerAddress);
  return withRetry(() => postLitAction(code, jsParams), { label: "lit_action" });
}

// Convenience: the action wallet address bound to this owner (no auth needed).
async function depositAddressFor(ownerAddress) {
  const out = await runUserAction(ownerAddress, { action: "address" });
  if (!out || !out.walletAddress) {
    throw new Error(`address derivation returned: ${JSON.stringify(out)}`);
  }
  return out.walletAddress;
}

// Poll the REAL execution path until the scoped usage key can run an action in
// the group. After add_usage_api_key, the key's execute-in-group grant takes a
// short, VARIABLE time to propagate — a fixed sleep either flakes or over-waits.
// Running the actual action (not a stand-in) is the only check that can't
// pass-then-fail, because it exercises the exact key + group + runtime path
// every demo script depends on. Returns the derived wallet address once ready.
async function waitForUsageKeyReady(ownerAddress, { timeoutMs = 60000, intervalMs = 2000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  for (let attempt = 1; ; attempt++) {
    try {
      return await depositAddressFor(ownerAddress);
    } catch (err) {
      if (Date.now() >= deadline) {
        throw new Error(`usage key still not usable after ${timeoutMs}ms: ${err.message}`);
      }
      console.log(`  usage key not ready yet (attempt ${attempt}: ${err.message}); waiting...`);
      await wait(intervalMs);
    }
  }
}

module.exports = { runUserAction, depositAddressFor, waitForUsageKeyReady };
