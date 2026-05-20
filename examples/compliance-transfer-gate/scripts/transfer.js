// End-to-end transfer:
//   1. Ask the Lit Action to screen the recipient against the Chainalysis
//      on-chain sanctions oracle and sign an authorization.
//   2. Submit `transferWithAuth` to the CompliantToken using the returned sig.
//
// Usage:
//   node scripts/transfer.js --to 0xRecipient --amount 100

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");
env.load();

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  // Scoped usage key created by setup.js (step 6) with execute permission
  // for the compliance action's group. Required for /lit_action — the
  // master account key won't work here.
  LIT_USAGE_API_KEY,
  COMPLIANT_TOKEN_ADDRESS,
  CHAIN_ID = "84532",
  RPC_URL = "https://sepolia.base.org",
  SENDER_PRIVATE_KEY,
  // Must be an eth-mainnet.g.alchemy.com URL — the action's hostname
  // whitelist is hardcoded. To use a different provider, edit
  // ALLOWED_SCREENING_HOST in action/complianceGate.js (which mints a
  // new action CID and signer address — you'll need to redeploy the
  // CompliantToken with the new oracle address).
  SCREENING_RPC_URL,
} = process.env;

function parseArgs() {
  const out = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    out[process.argv[i].replace(/^--/, "")] = process.argv[i + 1];
  }
  return out;
}

async function main() {
  const args = parseArgs();
  if (!args.to || !args.amount) {
    throw new Error("Usage: node scripts/transfer.js --to 0x... --amount 100");
  }
  for (const k of [
    "LIT_USAGE_API_KEY",
    "COMPLIANT_TOKEN_ADDRESS",
    "SENDER_PRIVATE_KEY",
    "SCREENING_RPC_URL",
  ]) {
    if (!process.env[k]) throw new Error(`${k} env var is required`);
  }

  const provider = new ethers.providers.JsonRpcProvider(RPC_URL);
  const sender = new ethers.Wallet(SENDER_PRIVATE_KEY, provider);
  const amount = ethers.utils.parseUnits(args.amount, 18).toString();
  const nonce = ethers.utils.hexlify(ethers.utils.randomBytes(32));
  const deadline = Math.floor(Date.now() / 1000) + 600;

  const code = fs.readFileSync(
    path.join(__dirname, "..", "action", "complianceGate.js"),
    "utf8"
  );

  const litRes = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
    method: "POST",
    headers: {
      "X-Api-Key": LIT_USAGE_API_KEY,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      code,
      js_params: {
        from: sender.address,
        to: args.to,
        amount,
        nonce,
        deadline,
        contractAddress: COMPLIANT_TOKEN_ADDRESS,
        chainId: Number(CHAIN_ID),
        screeningRpcUrl: SCREENING_RPC_URL,
      },
    }),
  });

  // /lit_action wraps the action's return value as
  //   { response: <whatever you returned>, logs: "...", has_error: bool }
  const envelope = await litRes.json();
  if (envelope.has_error) {
    console.error("Lit Action errored:", envelope.logs || envelope);
    process.exit(2);
  }
  const body = envelope.response;
  if (!body || !body.authorized) {
    console.error("Lit Action denied the transfer:", body || envelope);
    process.exit(2);
  }
  console.log("Recipient cleared sanctions screening. Submitting transfer...");

  const token = new ethers.Contract(
    COMPLIANT_TOKEN_ADDRESS,
    [
      "function transferWithAuth(address to, uint256 amount, bytes32 nonce, uint256 deadline, bytes signature) returns (bool)",
    ],
    sender
  );
  const tx = await token.transferWithAuth(
    args.to,
    amount,
    nonce,
    deadline,
    body.signature
  );
  console.log("tx:", tx.hash);
  const receipt = await tx.wait();
  console.log("mined in block", receipt.blockNumber);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
