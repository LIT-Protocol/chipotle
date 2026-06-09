// Shared constants + helpers for the CoW Protocol (Tier 1) integration.
//
// "Tier 1" = a self-deployed GPv2Settlement. CoW's canonical settlement is
// permissioned — only allowlisted (and on mainnet bonded) solvers may call
// settle() — so you can't run a self-serve test solve against it the way Across
// lets anyone fill. Instead we deploy our OWN GPv2Settlement +
// GPv2AllowListAuthentication (the real, audited contracts, from the published
// @cowprotocol/contracts artifacts) and allowlist our vault as the solver. The
// custody/policy story is exercised against the real settle() machinery and
// real EIP-712 order signatures — just on an instance we control.
//
// Because we deploy our own settlement, ANY EVM chain works. We use Base Sepolia
// (fast ~2s blocks, cheap, and matches the Across sibling); override COW_CHAIN_ID
// + the RPC host whitelist in action/cowPolicy.js to run elsewhere.

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");

const CHAIN_ID = Number(process.env.COW_CHAIN_ID || 84532); // Base Sepolia

// Balancer V2 Vault. GPv2Settlement takes a vault in its constructor and creates
// its VaultRelayer with it, but for plain erc20-balance orders (all we use) the
// vault is never called — payout is a direct safeTransfer from the settlement.
// The canonical Balancer address is the same across chains; override if needed.
const BALANCER_VAULT =
  process.env.BALANCER_VAULT || "0xBA12222222228d8Ba445958a75a0704d566BF2C8";

// Load a deployable artifact (abi + bytecode) from the @cowprotocol/contracts
// package — these are the real compiled GPv2 contracts.
function cowArtifact(name) {
  return require(`@cowprotocol/contracts/lib/contracts/${name}.json`);
}

const AUTH_ABI = [
  "function initializeManager(address manager_)",
  "function addSolver(address solver)",
  "function removeSolver(address solver)",
  "function isSolver(address prospectiveSolver) view returns (bool)",
  "function manager() view returns (address)",
];

const SETTLEMENT_ABI = [
  "function settle(address[] tokens, uint256[] clearingPrices, (uint256 sellTokenIndex,uint256 buyTokenIndex,address receiver,uint256 sellAmount,uint256 buyAmount,uint32 validTo,bytes32 appData,uint256 feeAmount,uint256 flags,uint256 executedAmount,bytes signature)[] trades, (address target,uint256 value,bytes callData)[][3] interactions)",
  "function domainSeparator() view returns (bytes32)",
  "function vaultRelayer() view returns (address)",
  "function authenticator() view returns (address)",
];

const VAULT_ABI = [
  "function executeSettlement(bytes settleCalldata, address pullToken, uint256 pullAmount, uint256 authDeadline, bytes signature)",
  "function settlement() view returns (address)",
  "function policySigner() view returns (address)",
  "function killSwitch() view returns (bool)",
  "function maxFillAmount() view returns (uint256)",
  "function setKillSwitch(bool on)",
  "function setMaxFillAmount(uint256 amount)",
  "function exit(address token)",
  "function owner() view returns (address)",
  "error InvalidPolicySignature()",
  "error AuthExpired()",
  "error KillSwitchEngaged()",
  "error OverCap()",
];

const MOCK_ERC20_ABI = [
  "function mint(address to, uint256 amount)",
  "function approve(address spender, uint256 amount) returns (bool)",
  "function allowance(address owner, address spender) view returns (uint256)",
  "function balanceOf(address) view returns (uint256)",
  "function transfer(address to, uint256 amount) returns (bool)",
  "function decimals() view returns (uint8)",
  "function symbol() view returns (string)",
];

// EIP-712 typed-data for a CoW order. kind / balances are strings in the type.
const ORDER_TYPES = {
  Order: [
    { name: "sellToken", type: "address" },
    { name: "buyToken", type: "address" },
    { name: "receiver", type: "address" },
    { name: "sellAmount", type: "uint256" },
    { name: "buyAmount", type: "uint256" },
    { name: "validTo", type: "uint32" },
    { name: "appData", type: "bytes32" },
    { name: "feeAmount", type: "uint256" },
    { name: "kind", type: "string" },
    { name: "partiallyFillable", type: "bool" },
    { name: "sellTokenBalance", type: "string" },
    { name: "buyTokenBalance", type: "string" },
  ],
};

function orderDomain(settlement) {
  return {
    name: "Gnosis Protocol",
    version: "v2",
    chainId: CHAIN_ID,
    verifyingContract: settlement,
  };
}

// Build the canonical sell / fill-or-kill / erc20 order this example uses.
function buildOrder({ sellToken, buyToken, receiver, sellAmount, buyAmount, validTo, appData }) {
  return {
    sellToken,
    buyToken,
    receiver,
    sellAmount: sellAmount.toString(),
    buyAmount: buyAmount.toString(),
    validTo: Number(validTo),
    appData: appData || ethers.constants.HashZero,
    feeAmount: "0",
    kind: "sell",
    partiallyFillable: false,
    sellTokenBalance: "erc20",
    buyTokenBalance: "erc20",
  };
}

// Sign the order (EIP-712, against the settlement's domain). Returns the 65-byte
// signature the trade carries; the settlement recovers the owner from it.
async function signOrder(wallet, settlement, order) {
  return wallet._signTypedData(orderDomain(settlement), ORDER_TYPES, order);
}

// The js_params the cowPolicy action expects: the trader's signed order plus
// where/what to settle.
function policyOrderParam(order, owner, signature) {
  return {
    sellToken: order.sellToken,
    buyToken: order.buyToken,
    receiver: order.receiver,
    sellAmount: order.sellAmount,
    buyAmount: order.buyAmount,
    validTo: order.validTo,
    appData: order.appData,
    feeAmount: order.feeAmount,
    owner,
    signature,
  };
}

// Ask the cowPolicy Lit Action to authorize a settlement. Returns the unwrapped
// response: { authorized, settleCalldata, pullToken, pullAmount, signature, ... }.
async function requestSolveAuthorization(jsParams) {
  const { LIT_API_BASE = "https://api.chipotle.litprotocol.com", COW_USAGE_API_KEY } = process.env;
  if (!COW_USAGE_API_KEY) {
    throw new Error("COW_USAGE_API_KEY is required (run `npm run setup`)");
  }
  const code = fs.readFileSync(path.join(__dirname, "..", "action", "cowPolicy.js"), "utf8");
  const res = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
    method: "POST",
    headers: { "X-Api-Key": COW_USAGE_API_KEY, "Content-Type": "application/json" },
    body: JSON.stringify({ code, js_params: jsParams }),
  });
  const envelope = await res.json();
  if (envelope.has_error) {
    throw new Error(`Lit Action errored: ${envelope.logs || JSON.stringify(envelope)}`);
  }
  return envelope.response;
}

module.exports = {
  CHAIN_ID,
  BALANCER_VAULT,
  cowArtifact,
  AUTH_ABI,
  SETTLEMENT_ABI,
  VAULT_ABI,
  MOCK_ERC20_ABI,
  ORDER_TYPES,
  orderDomain,
  buildOrder,
  signOrder,
  policyOrderParam,
  requestSolveAuthorization,
};
