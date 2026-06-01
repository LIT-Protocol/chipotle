// Shared helper: run a specific user's action-bound-wallet Lit Action.
//
// Each user has their own action source (their address stamped into the
// template), so this builds that source on the fly and executes it with the
// scoped usage key. Returns the unwrapped action response.

const { actionSourceFor } = require("./_users");

async function runUserAction(ownerAddress, jsParams) {
  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    LIT_USAGE_API_KEY,
  } = process.env;

  if (!LIT_USAGE_API_KEY) {
    throw new Error("LIT_USAGE_API_KEY is required (run `npm run setup`)");
  }

  const code = actionSourceFor(ownerAddress);

  const res = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
    method: "POST",
    headers: {
      "X-Api-Key": LIT_USAGE_API_KEY,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ code, js_params: jsParams }),
  });

  // /lit_action wraps the action's return as { response, logs, has_error }.
  const envelope = await res.json();
  if (envelope.has_error) {
    throw new Error(`Lit Action errored: ${envelope.logs || JSON.stringify(envelope)}`);
  }
  return envelope.response;
}

// Convenience: the action wallet address bound to this owner (no auth needed).
async function depositAddressFor(ownerAddress) {
  const out = await runUserAction(ownerAddress, { action: "address" });
  if (!out || !out.walletAddress) {
    throw new Error(`address derivation returned: ${JSON.stringify(out)}`);
  }
  return out.walletAddress;
}

module.exports = { runUserAction, depositAddressFor };
