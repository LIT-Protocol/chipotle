// Shared helper: ask the solverPolicy Lit Action to authorize a fill.
//
// This is the *only* call the solver bot makes to move inventory. It runs the
// policy action with a scoped usage key and returns the unwrapped action
// response: { authorized, signature, ... } on success, or { authorized:false,
// reason } when policy rejects. Callers decide what to do with each.

const fs = require("fs");
const path = require("path");

const ACTION_FILE = path.join(__dirname, "..", "action", "solverPolicy.js");

async function requestFillAuthorization(jsParams) {
  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    LIT_USAGE_API_KEY,
  } = process.env;

  if (!LIT_USAGE_API_KEY) {
    throw new Error("LIT_USAGE_API_KEY is required (run `npm run setup`)");
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

  // /lit_action wraps the action's return as { response, logs, has_error }.
  const envelope = await res.json();
  if (envelope.has_error) {
    throw new Error(`Lit Action errored: ${envelope.logs || JSON.stringify(envelope)}`);
  }
  return envelope.response;
}

// Build the js_params for a fill request from the deployed env + overrides.
function fillParams(overrides = {}) {
  const {
    SOLVER_VAULT_ADDRESS,
    MOCK_USDC_ADDRESS,
    MOCK_SETTLEMENT_ADDRESS,
    SAMPLE_DEPOSIT_ID,
    ORDER_RECIPIENT,
    ALCHEMY_BASE_SEPOLIA_URL,
  } = process.env;

  const { ethers } = require("ethers");
  const base = {
    vaultAddress: SOLVER_VAULT_ADDRESS,
    chainId: 84532,
    token: MOCK_USDC_ADDRESS,
    recipient: ORDER_RECIPIENT,
    amount: ethers.utils.parseUnits("100", 6).toString(),
    nonce: ethers.utils.hexlify(ethers.utils.randomBytes(32)),
    deadline: Math.floor(Date.now() / 1000) + 600,
    settlementContract: MOCK_SETTLEMENT_ADDRESS,
    depositId: SAMPLE_DEPOSIT_ID,
    rpcUrl: ALCHEMY_BASE_SEPOLIA_URL,
  };
  return { ...base, ...overrides };
}

module.exports = { requestFillAuthorization, fillParams };
