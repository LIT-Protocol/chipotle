// Watch a lit-triggers trigger's runs + the destination balance until the
// auto-relayed mint lands. Usage: node watchRuns.js <triggerId>

const fs = require("fs");
const os = require("os");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");

const BASE = "https://triggers.litprotocol.com";

async function main() {
  env.load();
  const triggerId = process.argv[2];
  if (!triggerId) throw new Error("usage: node watchRuns.js <triggerId>");
  const token = fs.readFileSync(path.join(os.homedir(), ".lit-triggers", "agent-token"), "utf8").trim();

  const A = process.env.ALCHEMY_API_KEY;
  const p = new ethers.providers.JsonRpcProvider(`https://arb-mainnet.g.alchemy.com/v2/${A}`);
  const t = new ethers.Contract(process.env.BRIDGE_TOKEN_ARB_MAINNET, ["function balanceOf(address) view returns (uint256)"], p);
  const dep = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY).address;

  for (let i = 0; i < 30; i++) {
    let runs = [];
    try {
      const r = await fetch(`${BASE}/api/triggers/${triggerId}/runs?limit=3`, { headers: { authorization: `Bearer ${token}` } });
      const j = await r.json();
      runs = Array.isArray(j) ? j : (j.runs || []); // API returns { runs: [...] }
    } catch (e) { /* transient */ }
    const latest = runs[0];
    const bal = await t.balanceOf(dep);
    console.log(`[t+${i * 10}s] latest run: ${latest ? latest.status : "none"} | Arb BRDG balance: ${ethers.utils.formatUnits(bal, 18)}`);
    if (bal.gt(0)) {
      console.log("\n✓ AUTO-RELAY CONFIRMED — the live lit-triggers poller ran the action, which minted on Arbitrum.");
      if (latest && latest.response) console.log("run response:", JSON.stringify(latest.response).slice(0, 400));
      return;
    }
    if (latest && latest.status === "failed") {
      console.log("run FAILED:", JSON.stringify(latest.error || latest.response || latest).slice(0, 600));
    }
    await new Promise((r) => setTimeout(r, 10000));
  }
  console.log("\n(timed out — check the trigger runs / lit-triggers BASE_RPC_URL config)");
}

main().catch((e) => { console.error("watch failed:", e.message); process.exit(1); });
