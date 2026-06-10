// Retry poller — recovers stuck burns the single-fire lit-triggers relayer
// missed (e.g. a burn that wasn't final, or the RPCs flaked, when the trigger
// fired its one time). Scans recent BurnInitiated events on each chain, checks
// whether each has already been minted on the destination (usedBurnIds), and
// re-runs the bridge action (relay mode) for any that are un-minted.
//
// Idempotent and safe to over-run: the on-chain usedBurnIds guard means a
// double-relay just reverts harmlessly, already-minted burns are skipped, and a
// not-yet-relayable burn (still pre-finality / underfunded prepay) is simply
// retried next cycle. This is the #4 follow-up (true finality) — the relayer can
// now wait for finality because the poller keeps retrying until the mint lands.
//
// Usage:
//   node retryPoller.js          # loop (RETRY_INTERVAL_SECS, default 120)
//   node retryPoller.js --once   # single pass (for cron)
// Env: RETRY_WINDOW_BLOCKS (default 50000), RETRY_INTERVAL_SECS (default 120).

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");
const lit = require("./lit");

const LIT_API_BASE = "https://api.chipotle.litprotocol.com";
const REGISTRY_READ_RPCS = ["https://base-rpc.publicnode.com", "https://1rpc.io/base", "https://gateway.tenderly.co/public/base"];
const BUILT_ACTION = path.join(__dirname, "..", "action", "bridgeAction.built.js");
const BURN_TOPIC = ethers.utils.id("BurnInitiated(address,address,uint256,uint256,uint256,uint256)");

function loadAbi(name) {
  return JSON.parse(fs.readFileSync(path.join(__dirname, "..", "contracts", "out", `${name}.sol`, `${name}.json`), "utf8")).abi;
}

// Chains this poller relays between. Mirrors the other scripts.
const CHAINS = {
  base: { id: 8453, name: "Base", tokenEnv: "BRIDGE_TOKEN_BASE_MAINNET", alchemySub: "base-mainnet" },
  arbitrum: { id: 42161, name: "Arbitrum", tokenEnv: "BRIDGE_TOKEN_ARB_MAINNET", alchemySub: "arb-mainnet" },
};

async function main() {
  env.load();
  const once = process.argv.includes("--once");
  const A = process.env.ALCHEMY_API_KEY;
  const usageKey = process.env.LIT_USAGE_API_KEY;
  if (!usageKey || !fs.existsSync(BUILT_ACTION)) throw new Error("run setup first (need LIT_USAGE_API_KEY + built action)");
  const actionCode = fs.readFileSync(BUILT_ACTION, "utf8");
  const windowBlocks = Number(process.env.RETRY_WINDOW_BLOCKS || 50000);
  const intervalSecs = Number(process.env.RETRY_INTERVAL_SECS || 120);
  const tokenAbi = loadAbi("BridgeToken");

  // Build providers + token handles keyed by chain id.
  const byId = {};
  for (const k of Object.keys(CHAINS)) {
    const c = CHAINS[k];
    const provider = new ethers.providers.JsonRpcProvider(`https://${c.alchemySub}.g.alchemy.com/v2/${A}`);
    byId[c.id] = { ...c, provider, token: new ethers.Contract(process.env[c.tokenEnv], tokenAbi, provider) };
  }

  async function pass() {
    for (const srcId of Object.keys(byId)) {
      const src = byId[srcId];
      let head, logs;
      try {
        head = await src.provider.getBlockNumber();
        logs = await src.provider.getLogs({
          address: src.token.address, topics: [BURN_TOPIC],
          fromBlock: Math.max(0, head - windowBlocks), toBlock: head,
        });
      } catch (e) {
        console.log(`[${src.name}] getLogs failed: ${e.message}`);
        continue;
      }
      for (const log of logs) {
        const destChainId = Number(ethers.BigNumber.from(log.topics[3]));
        const dest = byId[destChainId];
        if (!dest) continue; // destination chain not managed by this poller
        // burnId must match BridgeToken.mint: keccak256(abi.encode(srcChainId, burnTxHash, logIndex))
        const burnId = ethers.utils.keccak256(
          ethers.utils.defaultAbiCoder.encode(["uint256", "bytes32", "uint256"], [src.id, log.transactionHash, log.logIndex])
        );
        let minted;
        try {
          minted = await dest.token.usedBurnIds(burnId);
        } catch {
          continue; // dest RPC hiccup; try next cycle
        }
        if (minted) continue;

        console.log(`[retry] un-minted ${src.name}->${dest.name} tx ${log.transactionHash} log ${log.logIndex} — relaying...`);
        try {
          const resp = await lit.runAction(LIT_API_BASE, usageKey, actionCode, {
            source: "chain_event",
            event: {
              source: "chain_event", chain_id: src.id, transaction_hash: log.transactionHash,
              contract_address: src.token.address, log_index: log.logIndex,
            },
            registryRpcUrls: REGISTRY_READ_RPCS,
          });
          if (resp && resp.minted) console.log(`  ✓ minted on ${dest.name}: ${resp.mintTxHash}`);
          else console.log(`  not yet relayable: ${(resp && resp.reason) || JSON.stringify(resp)}`);
        } catch (e) {
          console.log(`  relay error: ${e.message}`);
        }
      }
    }
  }

  do {
    await pass();
    if (!once) await new Promise((r) => setTimeout(r, intervalSecs * 1000));
  } while (!once);
}

main().catch((e) => { console.error("retryPoller failed:", e.message); process.exit(1); });
