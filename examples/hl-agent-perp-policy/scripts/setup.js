// One-shot setup for the hl-agent-perp-policy example.
//
// What you provide (in .env before running):
//   LIT_API_KEY   Account-level (master) Lit API key
//
// What this script does, in order:
//   1. Compute the action's IPFS CID (lit-venues bundle + action source)
//   2. Create a permission group (wildcard action allowlist)
//   3. Create a scoped usage API key with execute_in_groups: [groupId]
//   4. Derive the agent address (runs the action's "address" branch)
//   5. Register the action with the account (metadata)
//   6. Add the specific action CID to the group (audit trail)
//
// There is no key to generate and nothing to deploy: the agent address IS the
// action, derived from its CID inside the TEE. The next step is venue-side —
// `npm run approve-agent` has your funded testnet master grant that address
// trade-only powers.
//
// Re-running this script does a fresh setup top-to-bottom: every step creates
// new state and overwrites the corresponding key in .env. The previously
// minted group / usage key become orphaned. That's fine for a docs example.
// NOTE: if the action source or the lit-venues bundle changed, the CID — and
// therefore the agent address — changes too; re-run `npm run approve-agent`.

const env = require("./_env");
const { runAction, buildCode } = require("./_lit");

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

  // -------------------------------------------------------------------------
  console.log("Step 1/6: Computing action CID (lit-venues bundle + action source)...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, buildCode());
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
  console.log("Step 4/6: Deriving the agent address...");
  // Use the scoped key we just minted to run the action's "address" branch.
  // This is the first use of a brand-new usage key, whose execute-in-group
  // grant is eventually consistent — so poll with retries until it propagates
  // rather than aborting setup (and half-populating .env) on a transient miss.
  const { agentAddress } = await runAction(
    { action: "address" },
    { retries: 10, delayMs: 3000 }
  );
  if (!agentAddress) throw new Error("action did not return an agent address");
  env.upsert("AGENT_ADDRESS", agentAddress);
  console.log(`  AGENT_ADDRESS=${agentAddress}`);

  // -------------------------------------------------------------------------
  console.log("Step 5/6: Registering action with account...");
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);

  // -------------------------------------------------------------------------
  console.log("Step 6/6: Adding action to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Action CID:   ", process.env.ACTION_IPFS_CID);
  console.log("  Agent address:", process.env.AGENT_ADDRESS);
  console.log("  Group ID:     ", process.env.GROUP_ID);
  console.log("\nNext:");
  console.log("  1. Fund your testnet master at https://app.hyperliquid-testnet.xyz/drip");
  console.log("  2. npm run approve-agent              # master grants the agent trade-only powers");
  console.log("  3. npm run trade -- buy 0.01 ETH      # policy-fenced market order");
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
      group_name: "hl-agent-perp-policy",
      group_description:
        "Policy-fenced Hyperliquid perp trading via a CID-bound agent wallet (plan D8)",
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
      name: "hlPerpPolicy",
      description:
        "PKP-native Hyperliquid agent: enforces coin allowlist, leverage, notional and reduce-only fences, then signs the order in-TEE",
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
      name: "hl-agent-perp-policy-executor",
      description:
        "Scoped key used by the demo scripts to execute the perp policy action",
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
