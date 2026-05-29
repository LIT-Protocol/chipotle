// One-shot setup for the chainlink-feed-mirror example.
//
// What you provide (in .env before running):
//   LIT_API_KEY            Account-level (master) Lit API key
//   DEPLOYER_PRIVATE_KEY   EOA with gas on Base Sepolia (deploys + funds relayer)
//
// What this does, in order:
//   1. Compute the action's IPFS CID
//   2. Create a permission group (wildcard action allowlist)
//   3. Create a scoped usage API key (execute_in_groups: [groupId])
//   4. Derive the action's wallet — the keyless relayer / consumer updater
//   5. Register the action with the account (metadata)
//   6. Add the action CID to the group (audit trail)
//   7. Fund the relayer wallet with gas on the destination chain
//   8. Deploy PriceConsumer on the destination chain, pinning relayer as updater
//   9. Resolve the Chainlink aggregator (it emits AnswerUpdated) from its proxy
//  10. Authorize this machine with lit-triggers (browser handshake)
//  11. Create the CHAIN_EVENT trigger (source aggregator + dest config)

const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { execSync } = require("child_process");
const { ethers } = require("ethers");
const env = require("./_env");

const ACTION_FILE = path.join(__dirname, "..", "action", "feedMirror.js");
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
    DEST_CHAIN_ID = "84532",
    FEED_SOURCE_CHAIN = "base",
    FEED_SOURCE_RPC = "https://mainnet.base.org",
    // Chainlink ETH/USD proxy on Base mainnet.
    FEED_SOURCE_PROXY = "0x71041dddad3595F9CEd3DcCFBe3D1F4b0a16Bb70",
    RELAYER_FUND_ETH = "0.0005",
  } = process.env;

  for (const k of ["LIT_API_KEY", "DEPLOYER_PRIVATE_KEY"]) {
    if (!process.env[k]) throw new Error(`${k} is required in .env.`);
  }
  const actionCode = fs.readFileSync(ACTION_FILE, "utf8");

  console.log("Step 1/11: Computing action CID...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, actionCode);
  env.upsert("ACTION_IPFS_CID", actionCid);
  console.log(`  ACTION_IPFS_CID=${actionCid}`);

  console.log("Step 2/11: Creating group...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  console.log("Step 3/11: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(`  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (written to .env)`);

  console.log("Step 4/11: Deriving relayer wallet from CID...");
  const relayer = await deriveActionWalletAddress(LIT_API_BASE, usageKey, actionCid);
  env.upsert("ACTION_WALLET_ADDRESS", relayer);
  console.log(`  ACTION_WALLET_ADDRESS=${relayer}`);

  console.log("Step 5/11: Registering action...");
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);

  console.log("Step 6/11: Adding action to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);

  console.log(`Step 7/11: Funding relayer with ${RELAYER_FUND_ETH} ETH for gas...`);
  await fundWallet(BASE_SEPOLIA_RPC_URL, relayer, RELAYER_FUND_ETH);

  console.log("Step 8/11: Deploying PriceConsumer (updater = relayer)...");
  delete process.env.PRICE_CONSUMER_BASE_SEPOLIA;
  execSync("npx hardhat run scripts/deploy.js --network baseSepolia", {
    stdio: "inherit",
    cwd: path.join(__dirname, ".."),
  });
  env.load();
  const consumer = process.env.PRICE_CONSUMER_BASE_SEPOLIA;

  console.log("Step 9/11: Resolving Chainlink aggregator from proxy...");
  const aggregator = await resolveAggregator(FEED_SOURCE_RPC, FEED_SOURCE_PROXY);
  env.upsert("FEED_AGGREGATOR", aggregator);
  console.log(`  aggregator=${aggregator}`);

  console.log("Step 10/11: Authorizing this machine with lit-triggers...");
  const agentToken = await authorizeAgent(TRIGGERS_BASE);
  env.upsert("LIT_TRIGGERS_AGENT_TOKEN", agentToken);

  console.log("Step 11/11: Creating the chain-event trigger...");
  const trigger = await createTrigger(TRIGGERS_BASE, agentToken, {
    name: "chainlink-feed-mirror",
    kind: "chain_event",
    action_code: actionCode,
    default_params: {
      destRpcUrl: BASE_SEPOLIA_RPC_URL,
      destChainId: DEST_CHAIN_ID,
      consumer,
      gasLimit: "150000",
      dryRun: false,
    },
    usage_api_key: usageKey,
    config: {
      chain: FEED_SOURCE_CHAIN,
      contract_address: aggregator,
      event_signature: "AnswerUpdated(int256,uint256,uint256)",
    },
  });
  env.upsert("TRIGGER_ID", trigger.id);

  console.log("\n✓ Setup complete.\n");
  console.log("  Action CID:      ", process.env.ACTION_IPFS_CID);
  console.log("  Relayer wallet:  ", relayer);
  console.log("  PriceConsumer:   ", consumer, `(dest chain ${DEST_CHAIN_ID})`);
  console.log("  Source feed:     ", `${FEED_SOURCE_CHAIN} aggregator ${aggregator}`);
  console.log("  Trigger:         ", trigger.id);
  console.log("\nThe trigger fires on the next on-chain AnswerUpdated (can take minutes).");
  console.log("For an immediate, deterministic check of the relay logic:");
  console.log("  npm run mirror -- --simulate");
}

// --- on-chain ---
async function fundWallet(rpcUrl, to, amountEth) {
  const provider = new ethers.providers.JsonRpcProvider(rpcUrl);
  const signer = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, provider);
  const have = await provider.getBalance(to);
  const want = ethers.utils.parseEther(amountEth);
  if (have.gte(want)) {
    console.log(`  relayer already has ${ethers.utils.formatEther(have)} ETH, skipping`);
    return;
  }
  const tx = await signer.sendTransaction({ to, value: want.sub(have) });
  console.log(`  funding tx: ${tx.hash}`);
  await tx.wait();
}
async function resolveAggregator(rpcUrl, proxy) {
  const provider = new ethers.providers.JsonRpcProvider(rpcUrl);
  const c = new ethers.Contract(proxy, ["function aggregator() view returns (address)"], provider);
  return c.aggregator();
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
  } catch { return false; }
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
  const MAX = 4;
  let lastErr;
  for (let attempt = 1; attempt <= MAX; attempt++) {
    try {
      const res = await fetch(`${base}/core/v1/${p}`, {
        ...init,
        headers: { "X-Api-Key": apiKey, "Content-Type": "application/json", ...(init.headers || {}) },
      });
      const text = await res.text();
      let body;
      try {
        body = text ? JSON.parse(text) : {};
      } catch {
        // Non-JSON body (e.g. an HTML 5xx error page). Transient — retry.
        lastErr = new Error(`${p} -> ${res.status}: non-JSON response`);
        if (res.status >= 500 && attempt < MAX) {
          await sleep(1000 * attempt);
          continue;
        }
        throw lastErr;
      }
      if (!res.ok) {
        const err = new Error(`${p} -> ${res.status}: ${body.message || body.error || JSON.stringify(body)}`);
        err.body = body;
        if (res.status >= 500 && attempt < MAX) {
          lastErr = err;
          await sleep(1000 * attempt);
          continue;
        }
        throw err;
      }
      return body;
    } catch (e) {
      // Network-level error — retry a few times.
      if (attempt < MAX && !e.body) {
        lastErr = e;
        await sleep(1000 * attempt);
        continue;
      }
      throw e;
    }
  }
  throw lastErr;
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
      group_name: "chainlink-feed-mirror",
      group_description: "Keyless relayer mirroring a Chainlink feed to another chain",
      pkp_ids_permitted: [],
      cid_hashes_permitted: ["0"],
    }),
  });
  return body.group_id;
}
async function addAction(base, apiKey, cid) {
  return call(base, apiKey, "add_action", {
    method: "POST",
    body: JSON.stringify({ action_ipfs_cid: cid, name: "feedMirror", description: "Mirror a Chainlink AnswerUpdated to a PriceConsumer on another chain" }),
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
      name: "chainlink-feed-mirror-executor",
      description: "Scoped key used by the chain-event trigger to execute the action",
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
