// Pure-node test harness that mirrors the consensus checks in
// action/consensusOracle.js. Skips the Lit envelope (no decryption, no PKP
// signing) so you can validate the multi-RPC logic before you have all three
// provider keys.
//
// Zero deps — uses only built-in fetch / URL / BigInt so you can run it
// without `npm install`.
//
// What it does:
//   1. Reads three plaintext RPC URLs from env (.env or process env).
//   2. Probes each for eth_chainId + eth_blockNumber.
//   3. Picks min(tip) - lag as the read block.
//   4. Calls eth_call (balanceOf) + eth_getBlockByNumber on all three in parallel.
//   5. Asserts byte-equality on returnData AND block hash.
//
// Usage:
//   RPC_URL_1=https://eth-mainnet.g.alchemy.com/v2/<key> \
//     node scripts/test-consensus.js
//
//   # or with custom token/holder
//   node scripts/test-consensus.js \
//     --token 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48 \
//     --holder 0x28C6c06298d514Db089934071355E5743bf21d60
//
// Defaults: USDC on Ethereum mainnet, Binance hot wallet 14 as the holder
// (always non-zero balance, large enough to be obviously real).

const fs = require("fs");
const path = require("path");

const DEFAULTS = {
  token: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
  holder: "0x28C6c06298d514Db089934071355E5743bf21d60",
};

// Minimal .env loader so we don't pull in dotenv.
function loadDotenv() {
  const file = path.join(__dirname, "..", ".env");
  if (!fs.existsSync(file)) return;
  for (const line of fs.readFileSync(file, "utf8").split("\n")) {
    const m = line.match(/^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*?)\s*$/);
    if (!m) continue;
    if (!process.env[m[1]]) process.env[m[1]] = m[2].replace(/^["']|["']$/g, "");
  }
}

function parseArgs() {
  const out = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    out[process.argv[i].replace(/^--/, "")] = process.argv[i + 1];
  }
  return out;
}

function encodeBalanceOfCalldata(holder) {
  const addr = holder.toLowerCase().replace(/^0x/, "");
  if (addr.length !== 40) throw new Error("invalid holder address");
  // selector(balanceOf(address)) = 0x70a08231
  return "0x70a08231" + "0".repeat(24) + addr;
}

async function rpc(url, method, params) {
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  const body = await res.json();
  if (body.error) throw new Error(`${url} error: ${body.error.message}`);
  return body.result;
}

async function rpcBatch(url, calls) {
  return Promise.all(calls.map((c) => rpc(url, c.method, c.params)));
}

function hostnameOf(url) {
  try {
    return new URL(url).hostname;
  } catch {
    return "(invalid)";
  }
}

async function main() {
  loadDotenv();

  const args = { ...DEFAULTS, ...parseArgs() };
  const sourceChainId = Number(process.env.SOURCE_CHAIN_ID || "1");
  const blockLagBlocks = Number(process.env.BLOCK_LAG_BLOCKS || "5");

  const rpcUrls = [
    process.env.RPC_URL_1,
    process.env.RPC_URL_2,
    process.env.RPC_URL_3,
  ];
  if (rpcUrls.some((u) => !u)) {
    throw new Error(
      "RPC_URL_1, RPC_URL_2, RPC_URL_3 are all required in .env (plaintext for this test)"
    );
  }
  console.log("Providers:");
  rpcUrls.forEach((u, i) => console.log(`  ${i + 1}. ${hostnameOf(u)}`));

  const callData = encodeBalanceOfCalldata(args.holder);

  console.log("\nProbing for chain id + tip...");
  const probes = await Promise.all(
    rpcUrls.map((u) =>
      rpcBatch(u, [
        { method: "eth_chainId", params: [] },
        { method: "eth_blockNumber", params: [] },
      ])
    )
  );
  probes.forEach((p, i) => {
    console.log(
      `  ${hostnameOf(rpcUrls[i])}: chainId=${parseInt(p[0], 16)} blockNumber=${parseInt(p[1], 16)}`
    );
  });

  for (const [cid] of probes) {
    if (parseInt(cid, 16) !== sourceChainId) {
      throw new Error(
        `chain id mismatch: expected ${sourceChainId}, one provider returned ${parseInt(cid, 16)}`
      );
    }
  }
  const tips = probes.map(([, bn]) => parseInt(bn, 16));
  const blockNumber = Math.min(...tips) - blockLagBlocks;
  const blockTag = "0x" + blockNumber.toString(16);
  console.log(
    `\nReading at block ${blockNumber} (min(tip)=${Math.min(...tips)}, lag=${blockLagBlocks}).`
  );

  console.log("\nFetching balanceOf + block hash from all 3...");
  const reads = await Promise.all(
    rpcUrls.map((u) =>
      rpcBatch(u, [
        { method: "eth_call", params: [{ to: args.token, data: callData }, blockTag] },
        { method: "eth_getBlockByNumber", params: [blockTag, false] },
      ])
    )
  );
  const returnDatas = reads.map(([r]) => r);
  const blockHashes = reads.map(([, b]) => b && b.hash);
  reads.forEach((_, i) => {
    console.log(
      `  ${hostnameOf(rpcUrls[i])}: returnData=${returnDatas[i].slice(0, 18)}... blockHash=${blockHashes[i] && blockHashes[i].slice(0, 12)}...`
    );
  });

  const returnDataAgree = returnDatas.every((r) => r === returnDatas[0]);
  const blockHashAgree = blockHashes.every((h) => h && h === blockHashes[0]);

  console.log("\nConsensus:");
  console.log(`  returnData identical: ${returnDataAgree ? "yes ✓" : "no ✗"}`);
  console.log(`  blockHash identical:  ${blockHashAgree ? "yes ✓" : "no ✗"}`);

  if (!returnDataAgree || !blockHashAgree) {
    console.error("\nFAILED: providers disagree. Action would refuse to sign.");
    process.exit(1);
  }

  const balance = BigInt(returnDatas[0]).toString();
  const observedAt = parseInt(reads[0][1].timestamp, 16);
  console.log(
    `\nPASSED. balanceOf(${args.holder}) = ${balance} at block ${blockNumber} (observedAt=${observedAt}).`
  );
  console.log(
    "The Lit Action would have signed (target, callData, returnData, observedAt, ...) at this point."
  );
}

main().catch((err) => {
  console.error("\nERROR:", err.message);
  process.exit(1);
});
