// Relayer test: prove the action AUTO-BROADCASTS in relay mode, without a
// running lit-triggers instance. We burn on the source chain, then invoke the
// bridge action with a synthetic chain_event `event` shaped exactly like what
// lit-triggers' dispatcher would pass. The action re-verifies, signs, and sends
// the mint itself from the oracle account. lit-triggers (in production) just
// calls this same action on the BurnInitiated event — see registerTriggers.js.
//
// Usage: node relay.js [--from baseSepolia] [--to arbitrumSepolia] [--amount 10]

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");
const lit = require("./lit");

const BUILT_ACTION = path.join(__dirname, "..", "action", "bridgeAction.built.js");

function loadAbi(name) {
  const p = path.join(__dirname, "..", "contracts", "out", `${name}.sol`, `${name}.json`);
  return JSON.parse(fs.readFileSync(p, "utf8")).abi;
}
function arg(flag, def) {
  const i = process.argv.indexOf(flag);
  return i !== -1 ? process.argv[i + 1] : def;
}

const CHAINS = {
  base: { id: 8453, name: "Base", tokenEnv: "BRIDGE_TOKEN_BASE_MAINNET", alchemySub: "base-mainnet" },
  arbitrum: { id: 42161, name: "Arbitrum", tokenEnv: "BRIDGE_TOKEN_ARB_MAINNET", alchemySub: "arb-mainnet" },
};
const REGISTRY_READ_RPCS = ["https://base-rpc.publicnode.com", "https://1rpc.io/base", "https://gateway.tenderly.co/public/base"];

async function main() {
  env.load();
  const { LIT_API_BASE = "https://api.chipotle.litprotocol.com", LIT_USAGE_API_KEY } = process.env;
  if (!LIT_USAGE_API_KEY || !fs.existsSync(BUILT_ACTION)) throw new Error("run setup first");
  const actionCode = fs.readFileSync(BUILT_ACTION, "utf8");

  const src = CHAINS[arg("--from", "base")];
  const dst = CHAINS[arg("--to", "arbitrum")];
  const minConf = Math.max(Number(process.env.MIN_CONFIRMATIONS || 5), 2);
  const amount = ethers.utils.parseUnits(arg("--amount", "10"), 18);

  const A = process.env.ALCHEMY_API_KEY;
  const srcProvider = new ethers.providers.JsonRpcProvider(`https://${src.alchemySub}.g.alchemy.com/v2/${A}`);
  const dstProvider = new ethers.providers.JsonRpcProvider(`https://${dst.alchemySub}.g.alchemy.com/v2/${A}`);
  const srcSigner = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, srcProvider);
  const recipient = arg("--recipient", await srcSigner.getAddress());

  const tokenAbi = loadAbi("BridgeToken");
  const srcToken = new ethers.Contract(process.env[src.tokenEnv], tokenAbi, srcSigner);
  const dstToken = new ethers.Contract(process.env[dst.tokenEnv], tokenAbi, dstProvider);

  // 1. Burn ----------------------------------------------------------------
  const prepay = (await dstProvider.getGasPrice()).mul(300000).mul(2); // covers action's MINT_GAS_LIMIT (300k) + buffer
  console.log(`Burning ${arg("--amount", "10")} on ${src.name} -> ${dst.name}; gas prepay ${ethers.utils.formatEther(prepay)} ETH...`);
  const burnRcpt = await (await srcToken.burn(amount, dst.id, recipient, { value: prepay })).wait();
  const burnTopic = ethers.utils.id("BurnInitiated(address,address,uint256,uint256,uint256,uint256)");
  const log = burnRcpt.logs.find(
    (l) => l.address.toLowerCase() === srcToken.address.toLowerCase() &&
           (l.topics[0] || "").toLowerCase() === burnTopic.toLowerCase()
  );
  console.log(`  burn tx ${burnRcpt.transactionHash} log ${log.logIndex} (block ${burnRcpt.blockNumber})`);

  // 2. Wait for finality ---------------------------------------------------
  console.log(`  waiting for ${minConf} confirmations...`);
  while ((await srcProvider.getBlockNumber()) - burnRcpt.blockNumber < minConf) {
    await new Promise((r) => setTimeout(r, 2000));
  }

  // 3. Synthetic chain_event — exactly what lit-triggers' dispatcher passes --
  const event = {
    source: "chain_event",
    chain_id: src.id,
    contract_address: srcToken.address,
    transaction_hash: burnRcpt.transactionHash,
    log_index: log.logIndex,
  };
  const before = await dstToken.balanceOf(recipient);

  console.log("  invoking bridge action in RELAY mode (it verifies, signs, AND broadcasts)...");
  const resp = await lit.runAction(LIT_API_BASE, LIT_USAGE_API_KEY, actionCode, {
    source: "chain_event",
    event,
    registryRpcUrls: REGISTRY_READ_RPCS, // destination resolved from bridgePartner; gas capped by the action
  });
  if (!resp || !resp.minted) throw new Error(`relay did not mint: ${JSON.stringify(resp)}`);

  // The public dest RPC can lag the node the action minted through — wait for
  // the mint receipt to land here before reading the balance, else it's stale.
  for (let i = 0; i < 15; i++) {
    const r = await dstProvider.getTransactionReceipt(resp.mintTxHash);
    if (r && r.blockNumber) break;
    await new Promise((r) => setTimeout(r, 2000));
  }
  const after = await dstToken.balanceOf(recipient);
  console.log(`\n✓ Relay complete — the ACTION broadcast the mint itself.`);
  console.log(`  oracle: ${resp.signer}`);
  console.log(`  mint tx (on ${dst.name}): ${resp.mintTxHash}`);
  console.log(`  ${dst.name} balance: ${ethers.utils.formatUnits(before, 18)} -> ${ethers.utils.formatUnits(after, 18)}`);
}

main().catch((err) => {
  console.error("\nRelay failed:", err.message);
  if (err.body) console.error("Server said:", JSON.stringify(err.body));
  process.exit(1);
});
