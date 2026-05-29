// One-shot setup for the uptime-insurance example.
//
// What you provide (in .env before running):
//   LIT_API_KEY            Account-level (master) Lit API key
//   DEPLOYER_PRIVATE_KEY   EOA with gas on Base Sepolia (funds the pool wallet)
//   POLICYHOLDER           address paid out (defaults to the deployer)
//
// What this does, in order:
//   1. Compute the action's IPFS CID
//   2. Create a permission group (wildcard action allowlist)
//   3. Create a scoped usage API key (execute_in_groups: [groupId])
//   4. Derive the action's wallet address — this IS the insurance pool
//   5. Register the action with the account (metadata)
//   6. Add the action CID to the group (audit trail)
//   7. Fund the pool wallet (payout reserve + gas)
//   8. Authorize this machine with lit-triggers (browser handshake)
//   9. Create the SCHEDULE trigger (cron + status config in default_params)
//
// There is no contract: the "pool" is simply the action wallet's ETH balance,
// and a payout is a plain ETH transfer the action signs and broadcasts itself.

const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { execSync } = require("child_process");
const { ethers } = require("ethers");
const env = require("./_env");

const ACTION_FILE = path.join(__dirname, "..", "action", "uptimeInsurance.js");
const ADDRESS_DERIVER_CODE = `
  async function main({ ipfsId }) {
    const walletAddress = await Lit.Actions.getLitActionWalletAddress({ ipfsId });
    return { walletAddress };
  }
`;

async function main() {
  env.load();
  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    TRIGGERS_BASE = "https://triggers.litprotocol.com",
    LIT_API_KEY,
    BASE_SEPOLIA_RPC_URL = "https://sepolia.base.org",
    STATUS_URL = "https://status.anthropic.com/api/v2/summary.json",
    PAYOUT_WEI = "200000000000000",
    POOL_FUND_ETH = "0.001",
    CRON = "* * * * *",
    DEMO_FORCE_DOWN = "true",
  } = process.env;

  for (const k of ["LIT_API_KEY", "DEPLOYER_PRIVATE_KEY"]) {
    if (!process.env[k]) throw new Error(`${k} is required in .env.`);
  }
  const deployer = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY);
  const policyholder = process.env.POLICYHOLDER || deployer.address;

  const actionCode = fs.readFileSync(ACTION_FILE, "utf8");

  console.log("Step 1/9: Computing action CID...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, actionCode);
  env.upsert("ACTION_IPFS_CID", actionCid);
  console.log(`  ACTION_IPFS_CID=${actionCid}`);

  console.log("Step 2/9: Creating group...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  console.log("Step 3/9: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(`  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (written to .env)`);

  console.log("Step 4/9: Deriving pool wallet address from CID...");
  const poolAddr = await deriveActionWalletAddress(LIT_API_BASE, usageKey, actionCid);
  env.upsert("POOL_WALLET_ADDRESS", poolAddr);
  console.log(`  POOL_WALLET_ADDRESS=${poolAddr}`);

  console.log("Step 5/9: Registering action...");
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);

  console.log("Step 6/9: Adding action to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);

  console.log(`Step 7/9: Funding pool wallet with ${POOL_FUND_ETH} ETH...`);
  await fundWallet(BASE_SEPOLIA_RPC_URL, poolAddr, POOL_FUND_ETH);

  console.log("Step 8/9: Authorizing this machine with lit-triggers...");
  const agentToken = await authorizeAgent(TRIGGERS_BASE);
  env.upsert("LIT_TRIGGERS_AGENT_TOKEN", agentToken);

  console.log("Step 9/9: Creating the schedule trigger...");
  const defaultParams = {
    statusUrl: STATUS_URL,
    rpcUrl: BASE_SEPOLIA_RPC_URL,
    policyholder,
    payoutWei: PAYOUT_WEI,
    gasLimit: "21000",
    dryRun: false,
  };
  // For the demo, force the "down" branch so a payout reliably fires. Remove
  // this (set DEMO_FORCE_DOWN=false) for production — the real status drives it.
  if (DEMO_FORCE_DOWN === "true") defaultParams.test_indicator = "critical";

  const trigger = await createTrigger(TRIGGERS_BASE, agentToken, {
    name: "uptime-insurance",
    kind: "schedule",
    action_code: actionCode,
    default_params: defaultParams,
    usage_api_key: usageKey,
    config: { cron: CRON },
  });
  env.upsert("TRIGGER_ID", trigger.id);

  console.log("\n✓ Setup complete.\n");
  console.log("  Action CID:    ", process.env.ACTION_IPFS_CID);
  console.log("  Pool wallet:   ", poolAddr);
  console.log("  Policyholder:  ", policyholder);
  console.log("  Cron:          ", CRON, DEMO_FORCE_DOWN === "true" ? "(DEMO: forcing 'critical')" : "");
  console.log("  Trigger:       ", trigger.id);
  console.log("\nWatch a payout fire on the next tick:");
  console.log("  npm run claim");
}

// --- on-chain ---
async function fundWallet(rpcUrl, to, amountEth) {
  const provider = new ethers.providers.JsonRpcProvider(rpcUrl);
  const signer = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, provider);
  const have = await provider.getBalance(to);
  const want = ethers.utils.parseEther(amountEth);
  if (have.gte(want)) {
    console.log(`  pool already has ${ethers.utils.formatEther(have)} ETH, skipping`);
    return;
  }
  const tx = await signer.sendTransaction({ to, value: want.sub(have) });
  console.log(`  funding tx: ${tx.hash}`);
  await tx.wait();
}

// --- lit-triggers ---
async function authorizeAgent(base) {
  const existing = process.env.LIT_TRIGGERS_AGENT_TOKEN;
  if (existing && (await meOk(base, existing))) {
    console.log("  reusing existing authorized agent token");
    return existing;
  }
  const token = crypto.randomBytes(36).toString("base64url");
  const challenge = crypto.createHash("sha256").update(token).digest("base64url");
  const url = `${base}/agent/authorize?challenge=${encodeURIComponent(challenge)}`;
  console.log("\n  Opening the authorization page. Sign in if needed, then click");
  console.log("  \"Authorize agent\". Waiting for approval...\n");
  console.log(`  ${url}\n`);
  openBrowser(url);
  const deadline = Date.now() + 5 * 60 * 1000;
  while (Date.now() < deadline) {
    await sleep(3000);
    if (await meOk(base, token)) return token;
  }
  throw new Error("timed out waiting for agent authorization (5 min)");
}

async function meOk(base, token) {
  try {
    const res = await fetch(`${base}/api/me`, { headers: { authorization: `Bearer ${token}` } });
    return res.ok;
  } catch {
    return false;
  }
}

async function createTrigger(base, token, body) {
  const res = await fetch(`${base}/api/triggers`, {
    method: "POST",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const out = await res.json();
  if (!res.ok) throw new Error(`create trigger -> ${res.status}: ${JSON.stringify(out)}`);
  return out;
}

function openBrowser(url) {
  const cmd = process.platform === "darwin" ? "open" : process.platform === "win32" ? 'start ""' : "xdg-open";
  try { execSync(`${cmd} "${url}"`, { stdio: "ignore" }); } catch {}
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// --- Chipotle REST ---
async function call(base, apiKey, p, init = {}) {
  const res = await fetch(`${base}/core/v1/${p}`, {
    ...init,
    headers: { "X-Api-Key": apiKey, "Content-Type": "application/json", ...(init.headers || {}) },
  });
  const body = await res.json();
  if (!res.ok) {
    const err = new Error(`${p} -> ${res.status}: ${body.message || body.error || JSON.stringify(body)}`);
    err.body = body;
    throw err;
  }
  return body;
}
async function getActionCid(base, apiKey, code) {
  return call(base, apiKey, "get_lit_action_ipfs_id", { method: "POST", body: JSON.stringify(code) });
}
async function deriveActionWalletAddress(base, apiKey, cid) {
  const body = await call(base, apiKey, "lit_action", {
    method: "POST",
    body: JSON.stringify({ code: ADDRESS_DERIVER_CODE, js_params: { ipfsId: cid } }),
  });
  if (body.has_error || !body.response || !body.response.walletAddress) {
    throw new Error(`address derivation failed: ${JSON.stringify(body)}`);
  }
  return body.response.walletAddress;
}
async function addGroup(base, apiKey) {
  const body = await call(base, apiKey, "add_group", {
    method: "POST",
    body: JSON.stringify({
      group_name: "uptime-insurance",
      group_description: "Keyless pool that pays out when a monitored service is down",
      pkp_ids_permitted: [],
      cid_hashes_permitted: ["0"],
    }),
  });
  return body.group_id;
}
async function addAction(base, apiKey, cid) {
  return call(base, apiKey, "add_action", {
    method: "POST",
    body: JSON.stringify({ action_ipfs_cid: cid, name: "uptimeInsurance", description: "Parametric uptime-insurance payout" }),
  });
}
async function addActionToGroup(base, apiKey, groupId, cid) {
  return call(base, apiKey, "add_action_to_group", {
    method: "POST",
    body: JSON.stringify({ group_id: Number(groupId), action_ipfs_cid: cid }),
  });
}
async function createUsageApiKey(base, apiKey, groupId) {
  const body = await call(base, apiKey, "add_usage_api_key", {
    method: "POST",
    body: JSON.stringify({
      name: "uptime-insurance-executor",
      description: "Scoped key used by the schedule trigger to execute the action",
      can_create_groups: false, can_delete_groups: false, can_create_pkps: false,
      manage_ipfs_ids_in_groups: [], add_pkp_to_groups: [], remove_pkp_from_groups: [],
      execute_in_groups: [Number(groupId)],
    }),
  });
  if (!body.usage_api_key) throw new Error(`add_usage_api_key returned no key: ${JSON.stringify(body)}`);
  return body.usage_api_key;
}

main().catch((err) => {
  console.error("\nSetup failed:", err.message);
  if (err.body) console.error("Server said:", err.body);
  process.exit(1);
});
