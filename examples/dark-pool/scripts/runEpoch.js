// Close one epoch: run the match action, verify the signed result, and (if
// contracts are deployed) settle it on-chain.
//
//   npm run run-epoch -- --epoch 1
//
// The match happens entirely inside the matchEpoch action (decrypt the batch,
// compute the uniform clearing price, sign the fills). This script never sees
// an order's contents — only the resulting fills, which are public post-trade.

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");
const { runAction } = require("./lit");

const SETTLEMENT_ABI = [
  "function settleEpoch(uint256 epoch, uint256 clearingPx, (address trader,bool isBuy,uint256 quantity)[] fills, bytes signature)",
];

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : def;
}

function fillsDigest(epoch, pair, clearingPx, fills, settlement, chainId) {
  const pairHash = ethers.utils.keccak256(ethers.utils.toUtf8Bytes(pair));
  const tuples = fills.map((f) => [f.trader, f.isBuy, f.quantity]);
  const fillsHash = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(["tuple(address trader, bool isBuy, uint256 quantity)[]"], [tuples])
  );
  return ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["uint256", "bytes32", "uint256", "bytes32", "address", "uint256"],
      [String(epoch), pairHash, clearingPx, fillsHash, settlement, String(chainId)]
    )
  );
}

async function main() {
  env.load();
  const base = process.env.LIT_API_BASE || "https://api.chipotle.litprotocol.com";
  const usageKey = process.env.LIT_USAGE_API_KEY;
  const pkpId = process.env.VAULT_PKP_ADDRESS;
  const encryptedDbUrl = process.env.ENCRYPTED_DATABASE_URL;
  const pair = process.env.PAIR || "BASE/QUOTE";
  const matcher = process.env.MATCH_ACTION_ADDRESS;
  const chainId = Number(process.env.CHAIN_ID || "84532");
  const maxBatch = Number(process.env.MAX_BATCH || "200");
  const epoch = Number(arg("epoch", "1"));
  // Orders are signed bound to the settlement address, so the matcher must
  // authenticate against the real one (a zero-address fallback would reject
  // every order).
  const settlement = process.env.SETTLEMENT_ADDRESS;

  for (const [k, v] of [
    ["LIT_USAGE_API_KEY", usageKey],
    ["VAULT_PKP_ADDRESS", pkpId],
    ["ENCRYPTED_DATABASE_URL", encryptedDbUrl],
    ["MATCH_ACTION_ADDRESS", matcher],
    ["SETTLEMENT_ADDRESS", settlement],
  ]) {
    if (!v) throw new Error(`${k} missing — run \`npm run setup\` (with DEPLOYER_PRIVATE_KEY set) first`);
  }

  // 1. Match inside the enclave.
  const code = fs.readFileSync(path.join(__dirname, "..", "action", "matchEpoch.js"), "utf8");
  const res = await runAction(base, usageKey, code, {
    pkpId,
    encryptedDbUrl,
    epoch,
    pair,
    settlement,
    chainId,
    maxBatch,
  });

  console.log(
    `epoch ${epoch}: ${res.matchedOrders} authenticated orders` +
      (res.rejectedOrders ? `, ${res.rejectedOrders} rejected` : "") +
      `, clearing price ${ethers.utils.formatUnits(res.clearingPx, 18)}`
  );
  for (const f of res.fills) {
    console.log(`  ${f.isBuy ? "BUY " : "SELL"} ${ethers.utils.formatUnits(f.quantity, 18)} -> ${f.trader}`);
  }
  if (res.fills.length === 0) {
    console.log("  (no cross this epoch)");
  }

  // 2. Verify the matcher's signature locally (also a digest-parity check
  //    between this script and the action).
  const digest = fillsDigest(epoch, pair, res.clearingPx, res.fills, settlement, chainId);
  const recovered = ethers.utils.verifyMessage(ethers.utils.arrayify(digest), res.signature);
  if (recovered.toLowerCase() !== matcher.toLowerCase()) {
    throw new Error(`signature does not recover to the matcher (${recovered} != ${matcher})`);
  }
  console.log(`signature verified -> matcher ${recovered}`);

  // 3. Settle on-chain, if contracts are deployed and we hold a key.
  const key = process.env.DEPLOYER_PRIVATE_KEY;
  if (process.env.SETTLEMENT_ADDRESS && key) {
    const provider = new ethers.providers.JsonRpcProvider(process.env.RPC_URL);
    const w = new ethers.Wallet(key, provider);
    const sc = new ethers.Contract(process.env.SETTLEMENT_ADDRESS, SETTLEMENT_ABI, w);
    const tuples = res.fills.map((f) => [f.trader, f.isBuy, f.quantity]);
    const tx = await sc.settleEpoch(epoch, res.clearingPx, tuples, res.signature);
    const rcpt = await tx.wait();
    console.log(`settled on-chain: ${rcpt.transactionHash}`);

    // Flip the matched orders + epoch to settled (via the pinned markSettled
    // action, so this orchestrator never handles raw DB credentials).
    const markCode = fs.readFileSync(path.join(__dirname, "..", "action", "markSettled.js"), "utf8");
    await runAction(base, usageKey, markCode, {
      pkpId,
      encryptedDbUrl,
      ids: res.orderIds,
      epoch,
      pair,
      clearingPx: res.clearingPx,
      txHash: rcpt.transactionHash,
    });
    console.log("orders marked settled.");
  } else if (!process.env.SETTLEMENT_ADDRESS) {
    console.log("(no SETTLEMENT_ADDRESS — confidential-only; deploy contracts to settle on-chain)");
  }
}

main().catch((err) => {
  console.error("run-epoch failed:", err.message);
  if (err.body) console.error("server said:", err.body);
  process.exit(1);
});
