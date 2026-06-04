// One-shot setup for the mpc-signing-ecdsa example.
//
// What you provide (in .env before running):
//   LIT_API_KEY              Account-level (master) Lit API key
//
// Unlike the other examples, setup does NOT deploy a contract — the vault's
// signer is the address produced by the distributed key generation, which
// doesn't exist until you run `npm run keygen`. So the flow is:
//   npm run setup   ->  npm run keygen  ->  npm run deploy  ->  npm run sign
//
// What this script does, in order:
//   1. Mint a PKP — the Encrypt/Decrypt boundary that seals the action's MPC
//      session + keyshare to THIS action's CID. (It signs nothing itself.)
//   2. Compute the action's IPFS CID.
//   3. Create a permission group (wildcard action allowlist).
//   4. Authorize the PKP inside the group.
//   5. Create a scoped usage API key (execute_in_groups: [groupId]).
//   6. Register the action with the account (metadata).
//   7. Add the specific action CID to the group (audit trail).
//
// Re-running does a fresh setup top-to-bottom; previously-minted PKP / group /
// usage key become orphaned. Re-running setup invalidates any existing
// keyshare (it was sealed to the old PKP), so re-run keygen afterwards.

const fs = require("fs");
const path = require("path");
const env = require("./_env");

const ACTION_FILE = path.join(__dirname, "..", "action", "mpcSigner.js");

async function main() {
  env.load();

  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    LIT_API_KEY,
  } = process.env;

  if (!LIT_API_KEY) {
    throw new Error("LIT_API_KEY is required in .env. Copy .env.example to .env and fill it in.");
  }

  const actionCode = fs.readFileSync(ACTION_FILE, "utf8");

  // -------------------------------------------------------------------------
  // Step 1: Mint the PKP used as the Encrypt/Decrypt boundary.
  // -------------------------------------------------------------------------
  console.log("Step 1/7: Minting PKP (seal boundary for the action's keyshare)...");
  const pkpAddr = await mintPkp(LIT_API_BASE, LIT_API_KEY);
  env.upsert("MPC_PKP_ADDRESS", pkpAddr);
  console.log(`  MPC_PKP_ADDRESS=${pkpAddr}`);

  // -------------------------------------------------------------------------
  // Step 2: Compute the action's IPFS CID.
  // -------------------------------------------------------------------------
  console.log("Step 2/7: Computing action CID...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, actionCode);
  env.upsert("ACTION_IPFS_CID", actionCid);
  console.log(`  ACTION_IPFS_CID=${actionCid}`);

  // -------------------------------------------------------------------------
  // Step 3: Create the group with a wildcard action allowlist.
  // -------------------------------------------------------------------------
  console.log("Step 3/7: Creating group (wildcard action allowlist)...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  // -------------------------------------------------------------------------
  // Step 4: Authorize the PKP inside the group.
  // -------------------------------------------------------------------------
  console.log("Step 4/7: Adding PKP to group...");
  await addPkpToGroup(LIT_API_BASE, LIT_API_KEY, groupId, pkpAddr);

  // -------------------------------------------------------------------------
  // Step 5: Create a scoped usage API key.
  // -------------------------------------------------------------------------
  console.log("Step 5/7: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(`  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`);

  // -------------------------------------------------------------------------
  // Step 6: Register the action (metadata).
  // -------------------------------------------------------------------------
  console.log("Step 6/7: Registering action with account...");
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);

  // -------------------------------------------------------------------------
  // Step 7: Add the specific action CID to the group (audit trail).
  // -------------------------------------------------------------------------
  console.log("Step 7/7: Adding action to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  PKP (seal boundary):", process.env.MPC_PKP_ADDRESS);
  console.log("  Action CID:         ", process.env.ACTION_IPFS_CID);
  console.log("  Group ID:           ", process.env.GROUP_ID);
  console.log("\nNext:");
  console.log("  npm run keygen                       # interactive 2-of-3 DKG; prints your address");
  console.log("  npm run deploy:baseSepolia           # deploy the vault to that address");
  console.log("  npm run sign -- --to 0x.. --value 0  # hot + Lit sign + exec");
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

async function mintPkp(base, apiKey) {
  const res = await fetch(`${base}/core/v1/create_wallet`, {
    method: "GET",
    headers: { "X-Api-Key": apiKey },
  });
  const body = await res.json();
  if (!res.ok || !body.wallet_address) {
    throw new Error(`mint failed: ${JSON.stringify(body)}`);
  }
  return body.wallet_address;
}

async function getActionCid(base, apiKey, code) {
  return call(base, apiKey, "get_lit_action_ipfs_id", {
    method: "POST",
    body: JSON.stringify(code),
  });
}

async function addGroup(base, apiKey) {
  const body = await call(base, apiKey, "add_group", {
    method: "POST",
    body: JSON.stringify({
      group_name: "mpc-signing-ecdsa",
      group_description: "2-of-3 threshold-ECDSA: Lit Action + user (hot + cold recovery)",
      pkp_ids_permitted: [],
      cid_hashes_permitted: ["0"],
    }),
  });
  return body.group_id;
}

async function addPkpToGroup(base, apiKey, groupId, pkpAddress) {
  return call(base, apiKey, "add_pkp_to_group", {
    method: "POST",
    body: JSON.stringify({ group_id: Number(groupId), pkp_id: pkpAddress }),
  });
}

async function addAction(base, apiKey, cid) {
  return call(base, apiKey, "add_action", {
    method: "POST",
    body: JSON.stringify({
      action_ipfs_cid: cid,
      name: "mpcSigner",
      description: "Lit-side party of a 2-of-3 threshold-ECDSA (DKLs23) signer",
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
      name: "mpc-signing-ecdsa-executor",
      description: "Scoped key used by keygen.js / sign.js for /lit_action calls",
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
