// End-to-end submission:
//   1. Ask the Lit Action to fetch the price for `asset` from Coinbase,
//      Kraken, and Bitstamp; take the median; sign the result.
//   2. Submit the signed reading to the PriceOracle registry.
//
// Usage:
//   node scripts/submit.js --asset ETH

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");
env.load();

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_API_KEY,
  PRICE_ORACLE_ADDRESS,
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
  if (!args.asset) {
    throw new Error("Usage: node scripts/submit.js --asset ETH");
  }
  for (const k of [
    "LIT_API_KEY",
    "PRICE_ORACLE_ADDRESS",
    "SUBMITTER_PRIVATE_KEY",
  ]) {
    if (!process.env[k]) throw new Error(`${k} is required`);
  }

  const deadline = Math.floor(Date.now() / 1000) + 600;
  const code = fs.readFileSync(
    path.join(__dirname, "..", "action", "priceOracle.js"),
    "utf8"
  );

  const litRes = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
    method: "POST",
    headers: {
      "X-Api-Key": LIT_API_KEY,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      code,
      js_params: {
        asset: args.asset,
        registryAddress: PRICE_ORACLE_ADDRESS,
        registryChainId: Number(REGISTRY_CHAIN_ID),
        deadline,
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
    console.error("Action declined to sign:", body || envelope);
    process.exit(2);
  }
  console.log(
    `${args.asset}/USD = $${body.priceFloat.toFixed(2)} (median of ${body.sources.length}, spread ${body.spreadBps} bps)`
  );
  console.log("Sources:");
  body.sources.forEach((s) => console.log(`  ${s.name.padEnd(10)} $${s.price}`));
  if (body.failed.length) {
    console.log("Failed sources:");
    body.failed.forEach((f) => console.log(`  ${f.name.padEnd(10)} ${f.error}`));
  }
  console.log(`On-chain price: ${body.price} (decimals=${body.decimals})`);

  const provider = new ethers.providers.JsonRpcProvider(REGISTRY_RPC_URL);
  const submitter = new ethers.Wallet(SUBMITTER_PRIVATE_KEY, provider);
  const registry = new ethers.Contract(
    PRICE_ORACLE_ADDRESS,
    [
      "function submit(string asset, uint256 price, uint8 decimals, uint256 observedAt, uint256 deadline, bytes signature)",
    ],
    submitter
  );

  const tx = await registry.submit(
    args.asset,
    body.price,
    body.decimals,
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
