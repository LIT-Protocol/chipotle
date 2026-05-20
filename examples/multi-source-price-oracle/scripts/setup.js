// One-shot setup for the multi-source-price-oracle example.
//
// What you provide (in .env before running):
//   LIT_API_KEY        Account or usage API key from the dashboard.
//   DEPLOYER_PRIVATE_KEY     EOA used to deploy the PriceOracle registry.
//
// Notice what's *not* here: no PKP, no API keys, no encryption. The three
// price sources (Coinbase, Kraken, Bitstamp) are all keyless public HTTP
// endpoints. The signature is produced by the action's CID-derived key.
//
// What this script does, in order:
//   1. Compute the action's IPFS CID
//   2. Derive the action's wallet address from its CID
//   3. Create a permission group
//   4. Register the action
//   5. Authorize the action inside the group
//   6. Deploy PriceOracle (pinning ACTION_WALLET_ADDRESS as signer)
//
// Every step that produces a new value writes it into .env, so re-runs
// skip whatever's already done.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const env = require("./_env");

const ACTION_FILE = path.join(__dirname, "..", "action", "priceOracle.js");
const DEPLOY_NETWORK = process.env.DEPLOY_NETWORK || "baseSepolia";

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
    LIT_API_KEY,
  } = process.env;

  for (const k of ["LIT_API_KEY", "DEPLOYER_PRIVATE_KEY"]) {
    if (!process.env[k]) {
      throw new Error(
        `${k} is required in .env. Copy .env.example to .env and fill it in.`
      );
    }
  }

  // -------------------------------------------------------------------------
  // Step 1: Compute the action's IPFS CID.
  // -------------------------------------------------------------------------
  const actionCode = fs.readFileSync(ACTION_FILE, "utf8");
  const freshCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, actionCode);
  if (!process.env.ACTION_IPFS_CID || process.env.ACTION_IPFS_CID !== freshCid) {
    if (process.env.ACTION_IPFS_CID) {
      console.log("Step 1/6: action source changed — updating CID...");
      env.upsert("ACTION_WALLET_ADDRESS", "");
      env.upsert("GROUP_ID", "");
    } else {
      console.log("Step 1/6: Computing action CID...");
    }
    env.upsert("ACTION_IPFS_CID", freshCid);
    console.log(`  ACTION_IPFS_CID=${freshCid}`);
  } else {
    console.log(`Step 1/6: action CID unchanged (${freshCid}). Skipping.`);
  }
  const actionCid = process.env.ACTION_IPFS_CID;

  // -------------------------------------------------------------------------
  // Step 2: Derive the action's wallet address from its CID.
  // -------------------------------------------------------------------------
  if (!process.env.ACTION_WALLET_ADDRESS) {
    console.log("Step 2/6: Deriving action wallet address from CID...");
    const addr = await deriveActionWalletAddress(
      LIT_API_BASE,
      LIT_API_KEY,
      actionCid
    );
    env.upsert("ACTION_WALLET_ADDRESS", addr);
    console.log(`  ACTION_WALLET_ADDRESS=${addr}`);
  } else {
    console.log(
      `Step 2/6: action wallet address already in .env (${process.env.ACTION_WALLET_ADDRESS}). Skipping.`
    );
  }

  // -------------------------------------------------------------------------
  // Step 3: Create a permission group.
  // -------------------------------------------------------------------------
  if (!process.env.GROUP_ID) {
    console.log("Step 3/6: Creating group...");
    const id = await addGroup(LIT_API_BASE, LIT_API_KEY);
    env.upsert("GROUP_ID", String(id));
    console.log(`  GROUP_ID=${id}`);
  } else {
    console.log(`Step 3/6: group already in .env (${process.env.GROUP_ID}). Skipping.`);
  }
  const groupId = Number(process.env.GROUP_ID);

  // -------------------------------------------------------------------------
  // Step 4: Register the action.
  // -------------------------------------------------------------------------
  console.log("Step 4/6: Registering action with account...");
  await idempotent(
    () => addAction(LIT_API_BASE, LIT_API_KEY, actionCid),
    "action already registered"
  );

  // -------------------------------------------------------------------------
  // Step 5: Authorize the action inside the group.
  // -------------------------------------------------------------------------
  console.log("Step 5/7: Adding action to group...");
  await idempotent(
    () => addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid),
    "action already in group"
  );

  // -------------------------------------------------------------------------
  // Step 6: Create a scoped usage API key with execute permission in the
  // group. See compliance-transfer-gate/scripts/setup.js for the full
  // rationale: the master LIT_API_KEY can do management calls but can't
  // execute /lit_action for actions in your own groups.
  // -------------------------------------------------------------------------
  if (!process.env.LIT_USAGE_API_KEY) {
    console.log("Step 6/7: Creating scoped usage API key...");
    const key = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
    env.upsert("LIT_USAGE_API_KEY", key);
    console.log(`  LIT_USAGE_API_KEY=${key.slice(0, 12)}... (full key written to .env)`);
  } else {
    console.log("Step 6/7: usage API key already in .env. Skipping.");
  }

  // -------------------------------------------------------------------------
  // Step 7: Deploy PriceOracle.
  // -------------------------------------------------------------------------
  if (!process.env.PRICE_ORACLE_ADDRESS) {
    console.log(`Step 7/7: Deploying PriceOracle to ${DEPLOY_NETWORK}...`);
    execSync(`npx hardhat run scripts/deploy.js --network ${DEPLOY_NETWORK}`, {
      stdio: "inherit",
      cwd: path.join(__dirname, ".."),
    });
    env.load();
  } else {
    console.log(`Step 7/7: contract already deployed (${process.env.PRICE_ORACLE_ADDRESS}). Skipping.`);
  }

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Action CID:            ", process.env.ACTION_IPFS_CID);
  console.log("  Action wallet (signer):", process.env.ACTION_WALLET_ADDRESS);
  console.log("  Group ID:              ", process.env.GROUP_ID);
  console.log("  PriceOracle:           ", process.env.PRICE_ORACLE_ADDRESS);
  console.log("\nTry it out:");
  console.log("  npm run test-medianizer -- --asset ETH   # off-line dry run, no chain");
  console.log("  npm run submit -- --asset ETH            # full flow, on-chain");
}

// ---------------------------------------------------------------------------
// Lit Chipotle REST helpers.
// ---------------------------------------------------------------------------

async function call(base, apiKey, path, init = {}) {
  const res = await fetch(`${base}/core/v1/${path}`, {
    ...init,
    headers: {
      "X-Api-Key": apiKey,
      "Content-Type": "application/json",
      ...(init.headers || {}),
    },
  });
  const body = await res.json();
  if (!res.ok) {
    const msg = body.message || body.error || JSON.stringify(body);
    const err = new Error(`${path} -> ${res.status}: ${msg}`);
    err.status = res.status;
    err.body = body;
    throw err;
  }
  return body;
}

async function getActionCid(base, apiKey, code) {
  return call(base, apiKey, "get_lit_action_ipfs_id", {
    method: "POST",
    body: JSON.stringify(code),
  });
}

async function deriveActionWalletAddress(base, apiKey, cid) {
  // /lit_action wraps the action's return value as
  //   { response: <whatever you returned>, logs: "...", has_error: bool }
  // so the action's payload lives at body.response, not body itself.
  const body = await call(base, apiKey, "lit_action", {
    method: "POST",
    body: JSON.stringify({
      code: ADDRESS_DERIVER_CODE,
      js_params: { ipfsId: cid },
    }),
  });
  if (body.has_error) {
    throw new Error(`address derivation failed: ${body.logs || JSON.stringify(body)}`);
  }
  const result = body.response;
  if (!result || !result.walletAddress) {
    throw new Error(`address derivation returned: ${JSON.stringify(body)}`);
  }
  return result.walletAddress;
}

async function addGroup(base, apiKey) {
  const body = await call(base, apiKey, "add_group", {
    method: "POST",
    body: JSON.stringify({
      group_name: "multi-source-price-oracle",
      group_description: "Action-derived signer for median spot-price attestations",
      pkp_ids_permitted: [],
      cid_hashes_permitted: [],
    }),
  });
  return body.group_id;
}

async function addAction(base, apiKey, cid) {
  return call(base, apiKey, "add_action", {
    method: "POST",
    body: JSON.stringify({
      action_ipfs_cid: cid,
      name: "priceOracle",
      description: "Multi-source median price oracle (Coinbase / Kraken / Bitstamp)",
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
      name: "multi-source-price-oracle-executor",
      description: "Scoped key used by submit.js to execute the price oracle action",
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

async function idempotent(fn, label) {
  try {
    await fn();
  } catch (err) {
    const text = (err.message || "").toLowerCase();
    if (
      err.status === 409 ||
      text.includes("already") ||
      text.includes("exists") ||
      text.includes("duplicate")
    ) {
      console.log(`  (${label})`);
      return;
    }
    throw err;
  }
}

main().catch((err) => {
  console.error("\nSetup failed:", err.message);
  if (err.body) console.error("Server said:", err.body);
  process.exit(1);
});
