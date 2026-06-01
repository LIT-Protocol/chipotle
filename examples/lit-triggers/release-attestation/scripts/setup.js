// One-shot setup for the release-attestation example.
//
// What you provide (in .env before running):
//   LIT_API_KEY            Account-level (master) Lit API key
//   DEPLOYER_PRIVATE_KEY   EOA with gas on Base Sepolia (deploys + funds action)
//   RELEASE_WEBHOOK_SECRET GitHub webhook secret (generated if left blank)
//
// What this does, in order:
//   1. Compute the action's IPFS CID
//   2. Create a permission group (wildcard action allowlist)
//   3. Create a scoped usage API key (execute_in_groups: [groupId])
//   4. Derive the action's wallet address from its CID
//   5. Register the action with the account (metadata)
//   6. Add the action CID to the group (audit trail)
//   7. Fund the action wallet with gas (it broadcasts the attest tx itself)
//   8. Deploy ReleaseRegistry, pinning the action wallet as `attester`
//   9. Authorize this machine with lit-triggers (browser handshake)
//  10. Create the webhook trigger (scoped key + registry in default_params)
//
// Re-running does a fresh setup top-to-bottom; previously-minted group / key /
// contract become orphaned. Fine for an example.

const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { execSync } = require("child_process");
const { ethers } = require("ethers");
const env = require("./_env");

const ACTION_FILE = path.join(__dirname, "..", "action", "releaseAttestation.js");
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
    ACTION_WALLET_GAS_ETH = "0.0005",
  } = process.env;

  for (const k of ["LIT_API_KEY", "DEPLOYER_PRIVATE_KEY"]) {
    if (!process.env[k]) {
      throw new Error(`${k} is required in .env. Copy .env.example to .env and fill it in.`);
    }
  }
  // Generate a webhook secret if the user didn't supply one.
  if (!process.env.RELEASE_WEBHOOK_SECRET) {
    env.upsert("RELEASE_WEBHOOK_SECRET", crypto.randomBytes(24).toString("hex"));
  }

  const actionCode = fs.readFileSync(ACTION_FILE, "utf8");

  console.log("Step 1/10: Computing action CID...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, actionCode);
  env.upsert("ACTION_IPFS_CID", actionCid);
  console.log(`  ACTION_IPFS_CID=${actionCid}`);

  console.log("Step 2/10: Creating group (wildcard action allowlist)...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  console.log("Step 3/10: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(`  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (written to .env)`);

  console.log("Step 4/10: Deriving action wallet address from CID...");
  const actionAddr = await deriveActionWalletAddress(LIT_API_BASE, usageKey, actionCid);
  env.upsert("ACTION_WALLET_ADDRESS", actionAddr);
  console.log(`  ACTION_WALLET_ADDRESS=${actionAddr}`);

  console.log("Step 5/10: Registering action with account...");
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);

  console.log("Step 6/10: Adding action to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);

  console.log(`Step 7/10: Funding action wallet with ${ACTION_WALLET_GAS_ETH} ETH for gas...`);
  await fundActionWallet(BASE_SEPOLIA_RPC_URL, actionAddr, ACTION_WALLET_GAS_ETH);

  console.log("Step 8/10: Deploying ReleaseRegistry (attester = action wallet)...");
  delete process.env.RELEASE_REGISTRY_BASE_SEPOLIA;
  execSync("npx hardhat run scripts/deploy.js --network baseSepolia", {
    stdio: "inherit",
    cwd: path.join(__dirname, ".."),
  });
  env.load();
  const registry = process.env.RELEASE_REGISTRY_BASE_SEPOLIA;

  console.log("Step 9/10: Authorizing this machine with lit-triggers...");
  const agentToken = await authorizeAgent(TRIGGERS_BASE);
  env.upsert("LIT_TRIGGERS_AGENT_TOKEN", agentToken);

  console.log("Step 10/10: Creating the webhook trigger...");
  const trigger = await createTrigger(TRIGGERS_BASE, agentToken, {
    name: "release-attestation",
    kind: "webhook",
    action_code: actionCode,
    default_params: {
      secret: process.env.RELEASE_WEBHOOK_SECRET,
      rpcUrl: BASE_SEPOLIA_RPC_URL,
      registry,
      gasLimit: "200000",
      dryRun: false,
    },
    usage_api_key: usageKey,
    config: {},
  });
  env.upsert("TRIGGER_ID", trigger.id);
  const webhookUrl = `${TRIGGERS_BASE}/webhook/${trigger.id}`;
  env.upsert("WEBHOOK_URL", webhookUrl);

  console.log("\n✓ Setup complete.\n");
  console.log("  Action CID:        ", process.env.ACTION_IPFS_CID);
  console.log("  Action wallet:     ", process.env.ACTION_WALLET_ADDRESS);
  console.log("  ReleaseRegistry:   ", registry);
  console.log("  Webhook URL:       ", webhookUrl);
  console.log("\nTry it out:");
  console.log("  npm run attest                 # simulate a signed GitHub release delivery");
  console.log("\nOr point a real GitHub repo webhook (Settings -> Webhooks) at the URL above,");
  console.log("  content type application/json, secret = RELEASE_WEBHOOK_SECRET, events = Releases.");
}

// ---------------------------------------------------------------------------
// On-chain helpers
// ---------------------------------------------------------------------------

async function fundActionWallet(rpcUrl, to, amountEth) {
  const provider = new ethers.providers.JsonRpcProvider(rpcUrl);
  const signer = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, provider);
  const have = await provider.getBalance(to);
  const want = ethers.utils.parseEther(amountEth);
  if (have.gte(want)) {
    console.log(`  action wallet already has ${ethers.utils.formatEther(have)} ETH, skipping`);
    return;
  }
  const tx = await signer.sendTransaction({ to, value: want.sub(have) });
  console.log(`  funding tx: ${tx.hash}`);
  await tx.wait();
}

// ---------------------------------------------------------------------------
// lit-triggers helpers
// ---------------------------------------------------------------------------

async function authorizeAgent(base) {
  // Reuse an existing authorized token if one is already in .env.
  const existing = process.env.LIT_TRIGGERS_AGENT_TOKEN;
  if (existing && (await meOk(base, existing))) {
    console.log("  reusing existing authorized agent token");
    return existing;
  }
  const token = crypto.randomBytes(36).toString("base64url");
  const challenge = crypto.createHash("sha256").update(token).digest("base64url");
  const url = `${base}/agent/authorize?challenge=${encodeURIComponent(challenge)}`;

  console.log("\n  Opening the authorization page in your browser. Sign in if needed,");
  console.log("  then click \"Authorize agent\". Waiting for approval...\n");
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
  const cmd =
    process.platform === "darwin" ? "open" :
    process.platform === "win32" ? "start \"\"" : "xdg-open";
  try {
    execSync(`${cmd} "${url}"`, { stdio: "ignore" });
  } catch {
    /* fall back to the printed URL */
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---------------------------------------------------------------------------
// Chipotle REST helpers — same shape as the other examples'.
// ---------------------------------------------------------------------------

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
      group_name: "release-attestation",
      group_description: "Keyless signer for GitHub release attestations",
      pkp_ids_permitted: [],
      cid_hashes_permitted: ["0"],
    }),
  });
  return body.group_id;
}

async function addAction(base, apiKey, cid) {
  return call(base, apiKey, "add_action", {
    method: "POST",
    body: JSON.stringify({
      action_ipfs_cid: cid,
      name: "releaseAttestation",
      description: "Verifies a GitHub release webhook and anchors it on-chain",
    }),
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
      name: "release-attestation-executor",
      description: "Scoped key used by the lit-triggers webhook to execute the action",
      can_create_groups: false,
      can_delete_groups: false,
      can_create_pkps: false,
      manage_ipfs_ids_in_groups: [],
      add_pkp_to_groups: [],
      remove_pkp_from_groups: [],
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
