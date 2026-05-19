// End-to-end submission:
//   1. Ask the Lit Action to run consensus across three RPCs and sign the
//      reading.
//   2. Submit the signed reading to the on-chain ConsensusOracle registry.
//
// The example reads ERC-20 `balanceOf(holder)` of a token at a fixed holder
// address — change `target`, `iface`, and `args` below for any other view
// function.
//
// Usage:
//   node scripts/submit.js \
//     --token 0xTokenAddress \
//     --holder 0xHolderAddress

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
require("dotenv").config();

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_USAGE_API_KEY,
  DECRYPT_PKP_ADDRESS,
  CONSENSUS_ORACLE_ADDRESS,
  ENCRYPTED_INFURA_URL,
  ENCRYPTED_ALCHEMY_URL,
  ENCRYPTED_QUICKNODE_URL,
  SOURCE_CHAIN_ID = "1",
  REGISTRY_CHAIN_ID = "84532",
  REGISTRY_RPC_URL = "https://sepolia.base.org",
  SUBMITTER_PRIVATE_KEY,
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
  if (!args.token || !args.holder) {
    throw new Error(
      "Usage: node scripts/submit.js --token 0x... --holder 0x..."
    );
  }
  for (const k of [
    "LIT_USAGE_API_KEY",
    "DECRYPT_PKP_ADDRESS",
    "CONSENSUS_ORACLE_ADDRESS",
    "ENCRYPTED_INFURA_URL",
    "ENCRYPTED_ALCHEMY_URL",
    "ENCRYPTED_QUICKNODE_URL",
    "SUBMITTER_PRIVATE_KEY",
  ]) {
    if (!process.env[k]) throw new Error(`${k} env var is required`);
  }

  const iface = new ethers.utils.Interface([
    "function balanceOf(address) view returns (uint256)",
  ]);
  const callData = iface.encodeFunctionData("balanceOf", [args.holder]);

  const deadline = Math.floor(Date.now() / 1000) + 600;

  const code = fs.readFileSync(
    path.join(__dirname, "..", "action", "consensusOracle.js"),
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
        target: args.token,
        callData,
        sourceChainId: Number(SOURCE_CHAIN_ID),
        registryAddress: CONSENSUS_ORACLE_ADDRESS,
        registryChainId: Number(REGISTRY_CHAIN_ID),
        deadline,
        decryptPkpId: DECRYPT_PKP_ADDRESS,
        encryptedRpcUrls: [
          ENCRYPTED_INFURA_URL,
          ENCRYPTED_ALCHEMY_URL,
          ENCRYPTED_QUICKNODE_URL,
        ],
      },
    }),
  });

  const body = await litRes.json();
  if (!body.authorized) {
    console.error("Action declined to sign:", body);
    process.exit(2);
  }
  const [balance] = iface.decodeFunctionResult("balanceOf", body.returnData);
  console.log(
    `Consensus reached at block ${body.blockNumber}: balance = ${balance.toString()}`
  );

  const provider = new ethers.providers.JsonRpcProvider(REGISTRY_RPC_URL);
  const submitter = new ethers.Wallet(SUBMITTER_PRIVATE_KEY, provider);
  const registry = new ethers.Contract(
    CONSENSUS_ORACLE_ADDRESS,
    [
      "function submit(address target, bytes callData, bytes returnData, uint256 observedAt, uint256 deadline, bytes signature)",
    ],
    submitter
  );

  const tx = await registry.submit(
    args.token,
    callData,
    body.returnData,
    body.observedAt,
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
