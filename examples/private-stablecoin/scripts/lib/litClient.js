// Thin client helpers shared by demo.js and disclose.js.

const { ethers } = require("ethers");

// The PrivUSD ABI fragments the scripts need.
const PRIVUSD_ABI = [
  "function totalSupply() view returns (uint256)",
  "function reserveBacked() view returns (bool)",
  "function commitments(bytes32) view returns (bool)",
  "function nullifiers(bytes32) view returns (bool)",
  "function mint(address depositor, uint256 depositAmount, bytes32[] newCommitments, string[] encryptedBlobs, bytes32 nonce, uint256 deadline, bytes signature)",
  "function shieldedTransfer(bytes32[] inputNullifiers, bytes32[] outputCommitments, string[] encryptedBlobs, bytes32 nonce, uint256 deadline, bytes signature)",
  "function redeem(bytes32[] inputNullifiers, bytes32[] changeCommitments, string[] changeBlobs, uint256 withdrawAmount, address recipient, bytes32 nonce, uint256 deadline, bytes signature)",
  "event NoteCreated(bytes32 indexed commitment, string encryptedBlob)",
  "event NoteSpent(bytes32 indexed nullifier)",
];

const MOCK_USDC_ABI = [
  "function approve(address spender, uint256 amount) returns (bool)",
  "function balanceOf(address) view returns (uint256)",
  "function mint(address to, uint256 amount)",
];

// Execute a Lit Action by source and return its (ok) response, throwing on
// error or denial. /lit_action wraps the return value as
//   { response: <returned>, logs: "...", has_error: bool }
async function callAction({ base, usageKey, code, jsParams }) {
  const res = await fetch(`${base}/core/v1/lit_action`, {
    method: "POST",
    headers: { "X-Api-Key": usageKey, "Content-Type": "application/json" },
    body: JSON.stringify({ code, js_params: jsParams }),
  });
  const envelope = await res.json();
  if (envelope.has_error) {
    throw new Error(`Lit Action errored: ${envelope.logs || JSON.stringify(envelope)}`);
  }
  const r = envelope.response;
  if (!r || !r.ok) {
    throw new Error(`Lit Action denied: ${JSON.stringify(r || envelope)}`);
  }
  return r;
}

function rand32() {
  return ethers.utils.hexlify(ethers.utils.randomBytes(32));
}

function deadlineIn(seconds) {
  return Math.floor(Date.now() / 1000) + seconds;
}

module.exports = { PRIVUSD_ABI, MOCK_USDC_ABI, callAction, rand32, deadlineIn };
