// Shared constants + helpers for the Across testnet integration.
//
// Route: Sepolia (origin) -> Base Sepolia (destination), WETH both sides.
// Confirmed enabled via https://testnet.across.to/api/available-routes.
// Override any address via .env if Across redeploys.

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");

const ORIGIN_CHAIN_ID = Number(process.env.ACROSS_ORIGIN_CHAIN_ID || 11155111); // Sepolia
const DEST_CHAIN_ID = Number(process.env.ACROSS_DEST_CHAIN_ID || 84532); // Base Sepolia

const ORIGIN_SPOKE =
  process.env.ACROSS_ORIGIN_SPOKE || "0x5ef6C01E11889d86803e0B23e3cB3F9E9d97B662";
const DEST_SPOKE =
  process.env.ACROSS_DEST_SPOKE || "0x82B564983aE7274c86695917BBf8C99ECb6F0F8F";

const ORIGIN_WETH =
  process.env.ACROSS_ORIGIN_WETH || "0xfFf9976782d46CC05630D1f6eBAb18b2324d6B14";
const DEST_WETH =
  process.env.ACROSS_DEST_WETH || "0x4200000000000000000000000000000000000006";

const WETH_ABI = [
  "function deposit() payable",
  "function approve(address,uint256) returns (bool)",
  "function balanceOf(address) view returns (uint256)",
  "function allowance(address,address) view returns (uint256)",
];

const SPOKE_DEPOSIT_ABI = [
  // Legacy address-based entrypoint — still present on the deployed SpokePool,
  // converts internally and emits the bytes32 FundsDeposited event below.
  "function depositV3(address depositor, address recipient, address inputToken, address outputToken, uint256 inputAmount, uint256 outputAmount, uint256 destinationChainId, address exclusiveRelayer, uint32 quoteTimestamp, uint32 fillDeadline, uint32 exclusivityDeadline, bytes message)",
  "function depositQuoteTimeBuffer() view returns (uint32)",
  "function fillDeadlineBuffer() view returns (uint32)",
  "event FundsDeposited(bytes32 inputToken, bytes32 outputToken, uint256 inputAmount, uint256 outputAmount, uint256 indexed destinationChainId, uint256 indexed depositId, uint32 quoteTimestamp, uint32 fillDeadline, uint32 exclusivityDeadline, bytes32 indexed depositor, bytes32 recipient, bytes32 exclusiveRelayer, bytes message)",
];

const VAULT_FILL_ABI = [
  "function executeAcrossFill((address depositor,address recipient,address exclusiveRelayer,address inputToken,address outputToken,uint256 inputAmount,uint256 outputAmount,uint256 originChainId,uint32 depositId,uint32 fillDeadline,uint32 exclusivityDeadline,bytes message) relayData, uint256 repaymentChainId, uint256 authDeadline, bytes signature)",
  "error InvalidPolicySignature()",
  "error AuthExpired()",
];

// Tuple type matching ISpokePool.V3RelayData — used to forge an attacker
// signature in the attack script and (must) match the action's encoding.
const RELAY_DATA_TUPLE =
  "tuple(address depositor,address recipient,address exclusiveRelayer,address inputToken,address outputToken,uint256 inputAmount,uint256 outputAmount,uint256 originChainId,uint32 depositId,uint32 fillDeadline,uint32 exclusivityDeadline,bytes message)";

// Ask the acrossPolicy Lit Action to authorize a fill. Returns the unwrapped
// response: { authorized, signature, relayData, authDeadline, ... }.
async function requestAcrossAuthorization(jsParams) {
  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    ACROSS_USAGE_API_KEY,
  } = process.env;
  if (!ACROSS_USAGE_API_KEY) {
    throw new Error("ACROSS_USAGE_API_KEY is required (run `npm run across:setup`)");
  }
  const code = fs.readFileSync(path.join(__dirname, "..", "action", "acrossPolicy.js"), "utf8");
  const res = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
    method: "POST",
    headers: { "X-Api-Key": ACROSS_USAGE_API_KEY, "Content-Type": "application/json" },
    body: JSON.stringify({ code, js_params: jsParams }),
  });
  const envelope = await res.json();
  if (envelope.has_error) {
    throw new Error(`Lit Action errored: ${envelope.logs || JSON.stringify(envelope)}`);
  }
  return envelope.response;
}

// js_params for an authorization request, from env + overrides.
function authParams(overrides = {}) {
  return {
    vaultAddress: process.env.ACROSS_VAULT_ADDRESS,
    chainId: DEST_CHAIN_ID,
    originSpokePool: ORIGIN_SPOKE,
    originChainId: ORIGIN_CHAIN_ID,
    depositId: process.env.ACROSS_DEPOSIT_ID,
    repaymentChainId: DEST_CHAIN_ID,
    authDeadline: Math.floor(Date.now() / 1000) + 600,
    fromBlock: process.env.ACROSS_DEPOSIT_BLOCK
      ? Number(process.env.ACROSS_DEPOSIT_BLOCK)
      : undefined,
    originRpcUrl: process.env.ALCHEMY_ETH_SEPOLIA_URL,
    vaultRpcUrl: process.env.ALCHEMY_BASE_SEPOLIA_URL,
    ...overrides,
  };
}

// relayData object (from the action) -> positional tuple for ethers.
function relayTuple(r) {
  return [
    r.depositor,
    r.recipient,
    r.exclusiveRelayer,
    r.inputToken,
    r.outputToken,
    r.inputAmount,
    r.outputAmount,
    r.originChainId,
    r.depositId,
    r.fillDeadline,
    r.exclusivityDeadline,
    r.message,
  ];
}

module.exports = {
  ORIGIN_CHAIN_ID,
  DEST_CHAIN_ID,
  ORIGIN_SPOKE,
  DEST_SPOKE,
  ORIGIN_WETH,
  DEST_WETH,
  WETH_ABI,
  SPOKE_DEPOSIT_ABI,
  VAULT_FILL_ABI,
  RELAY_DATA_TUPLE,
  requestAcrossAuthorization,
  authParams,
  relayTuple,
};
