// End-to-end transfer: burn on the source chain, run the bridge action (which
// reads config from the registry, reaches M-of-N consensus across Alchemy +
// Infura, and signs), then submit the mint on the destination chain.
//
// Usage:
//   node bridge.js [--from baseSepolia|arbitrumSepolia] [--to ...] [--amount 25] [--recipient 0x..]
// Defaults: Base Sepolia -> Arbitrum Sepolia, 25 tokens, recipient = deployer.

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");
const lit = require("./lit");

const BUILT_ACTION = path.join(__dirname, "..", "action", "bridgeAction.built.js");

function loadArtifact(name) {
  const p = path.join(__dirname, "..", "contracts", "out", `${name}.sol`, `${name}.json`);
  return JSON.parse(fs.readFileSync(p, "utf8"));
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
  for (const k of ["DEPLOYER_PRIVATE_KEY", "LIT_USAGE_API_KEY", "REGISTRY_ADDRESS"]) {
    if (!process.env[k]) throw new Error(`${k} missing — run setup first`);
  }
  if (!fs.existsSync(BUILT_ACTION)) throw new Error("bridgeAction.built.js missing — run setup first");
  const actionCode = fs.readFileSync(BUILT_ACTION, "utf8");

  const fromKey = arg("--from", "base");
  const toKey = arg("--to", "arbitrum");
  const src = CHAINS[fromKey], dst = CHAINS[toKey];
  if (!src || !dst || src === dst) throw new Error("bad --from/--to");

  const minConf = Math.max(Number(process.env.MIN_CONFIRMATIONS || 5), 2);
  const amount = ethers.utils.parseUnits(arg("--amount", "25"), 18);

  const A = process.env.ALCHEMY_API_KEY;
  const srcProvider = new ethers.providers.JsonRpcProvider(`https://${src.alchemySub}.g.alchemy.com/v2/${A}`);
  const dstProvider = new ethers.providers.JsonRpcProvider(`https://${dst.alchemySub}.g.alchemy.com/v2/${A}`);
  const srcSigner = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, srcProvider);
  const dstSigner = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, dstProvider);
  const recipient = arg("--recipient", await srcSigner.getAddress());

  const tokenAbi = loadArtifact("BridgeToken").abi;
  const srcToken = new ethers.Contract(process.env[src.tokenEnv], tokenAbi, srcSigner);
  const dstToken = new ethers.Contract(process.env[dst.tokenEnv], tokenAbi, dstSigner);

  // 1. Burn ----------------------------------------------------------------
  console.log(`Burning ${arg("--amount", "25")} on ${src.name} -> ${dst.name} (recipient ${recipient})...`);
  const burnTx = await srcToken.burn(amount, dst.id, recipient);
  const burnRcpt = await burnTx.wait();
  console.log(`  burn tx: ${burnTx.hash} (block ${burnRcpt.blockNumber})`);

  // Find the BurnInitiated log's block-level logIndex.
  const burnTopic = ethers.utils.id("BurnInitiated(address,address,uint256,uint256,uint256,uint256)");
  const log = burnRcpt.logs.find(
    (l) => l.address.toLowerCase() === srcToken.address.toLowerCase() &&
           (l.topics[0] || "").toLowerCase() === burnTopic.toLowerCase()
  );
  if (!log) throw new Error("BurnInitiated log not found in receipt");
  const logIndex = log.logIndex;

  // 2. Wait for finality (the action checks each provider independently) ----
  console.log(`  waiting for ${minConf} confirmations...`);
  while ((await srcProvider.getBlockNumber()) - burnRcpt.blockNumber < minConf) {
    await new Promise((r) => setTimeout(r, 2000));
  }

  // 3. Run the bridge action -----------------------------------------------
  console.log("  running bridge action (consensus across Alchemy + Infura)...");
  const deadline = Math.floor(Date.now() / 1000) + 3600;
  // destChainId + destContract are resolved by the action from the source
  // token's bridgePartner (not passed in), so they can't be spoofed here.
  const resp = await lit.runAction(LIT_API_BASE, LIT_USAGE_API_KEY, actionCode, {
    burnTxHash: burnTx.hash,
    srcChainId: src.id,
    srcContract: srcToken.address,
    logIndex,
    deadline,
    registryRpcUrls: REGISTRY_READ_RPCS,
  });
  if (!resp || !resp.authorized) {
    throw new Error(`action did not authorize: ${JSON.stringify(resp)}`);
  }
  if (resp.destContract.toLowerCase() !== dstToken.address.toLowerCase()) {
    throw new Error(`action resolved destContract ${resp.destContract} != expected ${dstToken.address}`);
  }
  console.log(`  authorized by oracle ${resp.signer} (quorum ${resp.quorum}); dest ${resp.destContract}`);

  // 4. Mint on destination -------------------------------------------------
  console.log(`  submitting mint on ${dst.name}...`);
  const before = await dstToken.balanceOf(recipient);
  const mintTx = await dstToken.mint(
    src.id, srcToken.address, burnTx.hash, logIndex, recipient,
    resp.amount, resp.srcNonce, deadline, resp.signature
  );
  await mintTx.wait();
  const after = await dstToken.balanceOf(recipient);

  console.log(`  mint tx: ${mintTx.hash}`);
  console.log(`\n✓ Transfer complete.`);
  console.log(`  ${dst.name} balance: ${ethers.utils.formatUnits(before, 18)} -> ${ethers.utils.formatUnits(after, 18)}`);
}

main().catch((err) => {
  console.error("\nBridge failed:", err.message);
  if (err.body) console.error("Server said:", JSON.stringify(err.body));
  process.exit(1);
});
