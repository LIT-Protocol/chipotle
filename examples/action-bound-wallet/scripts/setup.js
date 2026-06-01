// One-shot setup for the action-bound-wallet example.
//
// What you provide (in .env before running):
//   LIT_API_KEY            Account-level (master) Lit API key
//   RPC_URL                Base-Sepolia RPC URL (used for reads + broadcasts)
//   DEPLOYER_PRIVATE_KEY   EOA with Base-Sepolia gas; deploys + funds the demo
//
// Why a wildcard group: every user gets a DIFFERENT action (their address is
// stamped in), so every user's action has a different CID. Rather than
// allowlist each one, we create a single group with a wildcard action
// allowlist (cid_hashes_permitted: ["0"]) and a scoped usage key that can
// execute inside that group. The usage key carries NO spending power — the
// action authorizes withdrawals by recovering the owner's signature, not by
// trusting whoever ran it.
//
// Steps, in order:
//   1. Create a permission group (wildcard action allowlist)
//   2. Create a scoped usage API key with execute_in_groups: [groupId]
//   3. Deploy the DemoToken ERC-20 (scripts/deploy.js)
//   4. Verify the usage key can actually execute an action in the group, by
//      polling the real path until it succeeds (the grant propagates with a
//      short, variable delay — see waitForUsageKeyReady in _lit.js)
//
// Re-running does a fresh setup and overwrites the derived values in .env; the
// previously-minted group/usage key/token become orphaned. Fine for a docs
// example — we optimize for "explain every endpoint," not cost.

const path = require("path");
const { execSync } = require("child_process");
const env = require("./_env");
const { userWallet } = require("./_users");
const { waitForUsageKeyReady } = require("./_lit");

const DEPLOY_NETWORK = process.env.DEPLOY_NETWORK || "baseSepolia";

async function main() {
  env.load();

  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    LIT_API_KEY,
  } = process.env;

  for (const k of ["LIT_API_KEY", "DEPLOYER_PRIVATE_KEY", "RPC_URL"]) {
    if (!process.env[k]) {
      throw new Error(`${k} is required in .env. Copy .env.example to .env and fill it in.`);
    }
  }

  console.log("Step 1/4: Creating group (wildcard action allowlist)...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  console.log("Step 2/4: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(`  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`);

  console.log(`Step 3/4: Deploying DemoToken on ${DEPLOY_NETWORK}...`);
  execSync(`npx hardhat run scripts/deploy.js --network ${DEPLOY_NETWORK}`, {
    stdio: "inherit",
    cwd: path.join(__dirname, ".."),
  });
  env.load();

  console.log("Step 4/4: Verifying the usage key can execute in the group...");
  // env.upsert already set process.env.LIT_USAGE_API_KEY, so this runs the real
  // action path with the key we just minted, retrying until the grant lands.
  const probeWallet = await waitForUsageKeyReady(userWallet(0).address);
  console.log(`  usage key is live (user 0 wallet derived: ${probeWallet})`);

  console.log("\n✓ Setup complete.\n");
  console.log("  Group ID:    ", process.env.GROUP_ID);
  console.log("  Demo token:  ", process.env.DEMO_TOKEN_ADDRESS);
  console.log("\nWalk the demo:");
  console.log("  npm run address -- 0           # user 0's action-bound deposit wallet");
  console.log("  npm run deposit -- 0 100       # fund it with 100 ABD + a little gas");
  console.log("  npm run balance -- 0           # check its balances");
  console.log("  npm run withdraw -- 0 <to> 25  # user 0 auths a 25-ABD withdrawal");
  console.log("  npm run attack:wrong-user -- 0 1   # user 1 tries to drain user 0 -> rejected");
}

// ---------------------------------------------------------------------------
// Lit Chipotle REST helpers.
// ---------------------------------------------------------------------------

// Retries transient failures: network errors (node's "fetch failed") and 5xx
// responses. The Lit API occasionally 500s on a management call (e.g.
// add_usage_api_key returning a null) and succeeds on an immediate retry, so we
// don't want the one-shot setup to abort on a blip. 4xx errors are permanent
// (bad key, bad payload) and surface right away.
async function call(base, apiKey, path, init = {}, attempts = 4) {
  let lastErr;
  for (let i = 1; i <= attempts; i++) {
    let res;
    try {
      res = await fetch(`${base}/core/v1/${path}`, {
        ...init,
        headers: {
          "X-Api-Key": apiKey,
          "Content-Type": "application/json",
          ...(init.headers || {}),
        },
      });
    } catch (err) {
      lastErr = err;
      if (i === attempts) throw err;
      console.warn(`  ${path} network error (${err.message}); retrying...`);
      await new Promise((r) => setTimeout(r, 1500 * i));
      continue;
    }

    const body = await res.json().catch(() => ({}));
    if (res.ok) return body;

    const msg = body.message || body.error || JSON.stringify(body);
    const err = new Error(`${path} -> ${res.status}: ${msg}`);
    err.status = res.status;
    err.body = body;
    if (res.status >= 500 && i < attempts) {
      lastErr = err;
      console.warn(`  ${path} -> ${res.status}; retrying...`);
      await new Promise((r) => setTimeout(r, 1500 * i));
      continue;
    }
    throw err;
  }
  throw lastErr;
}

async function addGroup(base, apiKey) {
  const body = await call(base, apiKey, "add_group", {
    method: "POST",
    body: JSON.stringify({
      group_name: "action-bound-wallet",
      group_description: "Per-user wallets bound to a Lit Action's CID",
      pkp_ids_permitted: [],
      // U256(0) = "any action allowed in this group." Each user's action has a
      // different CID (their address is stamped in), so we can't enumerate them
      // up front. Bounded by the scoped usage key (step 2).
      cid_hashes_permitted: ["0"],
    }),
  });
  return body.group_id;
}

async function createUsageApiKey(base, apiKey, groupId) {
  const body = await call(base, apiKey, "add_usage_api_key", {
    method: "POST",
    body: JSON.stringify({
      name: "action-bound-wallet-executor",
      description: "Scoped key that runs per-user action-bound wallets",
      can_create_groups: false,
      can_delete_groups: false,
      can_create_pkps: false,
      manage_ipfs_ids_in_groups: [],
      add_pkp_to_groups: [],
      remove_pkp_from_groups: [],
      execute_in_groups: [Number(groupId)],
    }),
  });
  if (!body.usage_api_key) {
    throw new Error(`add_usage_api_key returned no key: ${JSON.stringify(body)}`);
  }
  return body.usage_api_key;
}

main().catch((err) => {
  console.error("\nSetup failed:", err.message);
  if (err.body) console.error("Server said:", err.body);
  process.exit(1);
});
