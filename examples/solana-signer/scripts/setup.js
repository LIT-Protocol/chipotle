// One-shot setup for the solana-signer example.
//
// What you provide (in .env before running):
//   LIT_API_KEY   Account-level (master) Lit API key
//
// What this script does, in order:
//   1. Compute the action's IPFS CID
//   2. Create a permission group (wildcard action allowlist)
//   3. Create a scoped usage API key with execute_in_groups: [groupId]
//   4. Derive the action's Solana address (runs the action's "address" branch)
//   5. Register the action with the account (metadata)
//   6. Add the specific action CID to the group (audit trail)
//
// There is no contract to deploy: the Solana address IS the wallet, derived
// from the action's CID. Fund it with devnet SOL (`npm run airdrop`), then
// send from it (`npm run transfer`).
//
// Re-running this script does a fresh setup top-to-bottom: every step creates
// new state and overwrites the corresponding key in .env. The previously
// minted group / usage key become orphaned. That's fine for a docs example.

const fs = require("fs");
const env = require("./_env");
const { runAction, ACTION_FILE } = require("./_lit");

async function main() {
  env.load();

  const {
    LIT_API_BASE = "https://api.chipotle.litprotocol.com",
    LIT_API_KEY,
  } = process.env;

  if (!LIT_API_KEY) {
    throw new Error(
      "LIT_API_KEY is required in .env. Copy .env.example to .env and fill it in."
    );
  }

  const actionCode = fs.readFileSync(ACTION_FILE, "utf8");

  // -------------------------------------------------------------------------
  console.log("Step 1/6: Computing action CID...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, actionCode);
  env.upsert("ACTION_IPFS_CID", actionCid);
  console.log(`  ACTION_IPFS_CID=${actionCid}`);

  // -------------------------------------------------------------------------
  console.log("Step 2/6: Creating group (wildcard action allowlist)...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  // -------------------------------------------------------------------------
  console.log("Step 3/6: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(
    `  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`
  );

  // -------------------------------------------------------------------------
  console.log("Step 4/6: Deriving the action's Solana address...");
  // Use the scoped key we just minted to run the action's "address" branch.
  const { address } = await runAction({ action: "address" });
  if (!address) throw new Error("action did not return an address");
  env.upsert("SOLANA_ADDRESS", address);
  console.log(`  SOLANA_ADDRESS=${address}`);

  // -------------------------------------------------------------------------
  console.log("Step 5/6: Registering action with account...");
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);

  // -------------------------------------------------------------------------
  console.log("Step 6/6: Adding action to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Action CID:    ", process.env.ACTION_IPFS_CID);
  console.log("  Solana address:", process.env.SOLANA_ADDRESS);
  console.log("  Group ID:      ", process.env.GROUP_ID);
  console.log("\nTry it out:");
  console.log("  npm run address                       # re-derive the address");
  console.log("  npm run airdrop                        # fund it with devnet SOL");
  console.log("  npm run transfer -- <recipient> 0.01   # sign + send 0.01 SOL");
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

async function addGroup(base, apiKey) {
  const body = await call(base, apiKey, "add_group", {
    method: "POST",
    body: JSON.stringify({
      group_name: "solana-signer",
      group_description: "Action-derived keyless Solana wallet (ed25519)",
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
      name: "solanaSigner",
      description: "Keyless Solana wallet bound to the action CID; signs capped SystemProgram transfers",
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
      name: "solana-signer-executor",
      description: "Scoped key used by the demo scripts to execute the solana signer action",
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
