// One-shot setup for the price-trigger-stop example.
//
// What you provide (in .env before running):
//   LIT_API_KEY   Account-level (master) Lit API key
//
// What this script does, in order:
//   1. Compose bundle+action and compute the executable's IPFS CID
//   2. Create a permission group (wildcard action allowlist)
//   3. Create a scoped usage API key with execute_in_groups: [groupId]
//   4. Probe-run the composed action (proves the grant propagated AND that
//      the ~140KB lit-venues bundle parses + executes in the runtime)
//   5. Register the action with the account (metadata)
//   6. Add the specific action CID to the group (audit trail)
//   7. Generate TRIGGER_ID — the idempotency seed the stop's clientOrderId
//      is derived from (32 hex chars, so the hyperliquid cloid derivation
//      uses it verbatim)
//
// Arming the poller is a separate step (`npm run arm` via lit-triggers, or
// `npm run poll` locally) so you can choose the invocation mode.
//
// Re-running this script does a fresh setup top-to-bottom: every step creates
// new state and overwrites the corresponding key in .env. The previously
// minted group / usage key become orphaned. That's fine for a docs example.
// NOTE: rebuilding lit-venues changes the composed bytes and therefore the
// CID — re-run setup (and re-arm) afterwards so what's registered matches
// what executes.

const crypto = require("crypto");
const env = require("./_env");
const { runAction, composeCode } = require("./_lit");

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

  const code = composeCode(); // throws with build instructions if the bundle is missing

  // -------------------------------------------------------------------------
  console.log("Step 1/7: Computing action CID (lit-venues bundle + priceStop.js)...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, code);
  env.upsert("ACTION_IPFS_CID", actionCid);
  console.log(`  ACTION_IPFS_CID=${actionCid}`);

  // -------------------------------------------------------------------------
  console.log("Step 2/7: Creating group (wildcard action allowlist)...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  // -------------------------------------------------------------------------
  console.log("Step 3/7: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(
    `  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`
  );

  // -------------------------------------------------------------------------
  console.log("Step 4/7: Probe-running the action (grant propagation + bundle check)...");
  // First use of the brand-new usage key: its execute-in-group grant is
  // eventually consistent, so poll with retries rather than aborting setup.
  const probe = await runAction({ probe: true }, { retries: 10, delayMs: 3000 });
  if (!probe || !probe.ok) throw new Error(`probe failed: ${JSON.stringify(probe)}`);
  console.log(`  lit-venues v${probe.litVenuesVersion} executes in-runtime`);

  // -------------------------------------------------------------------------
  console.log("Step 5/7: Registering action with account...");
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);

  // -------------------------------------------------------------------------
  console.log("Step 6/7: Adding action to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);

  // -------------------------------------------------------------------------
  console.log("Step 7/7: Generating TRIGGER_ID (idempotency seed)...");
  if (!process.env.TRIGGER_ID) {
    env.upsert("TRIGGER_ID", crypto.randomBytes(16).toString("hex"));
  }
  console.log(`  TRIGGER_ID=${process.env.TRIGGER_ID}`);

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Action CID:", process.env.ACTION_IPFS_CID);
  console.log("  Group ID:  ", process.env.GROUP_ID);
  console.log("\nNext: set the stop policy + venue credentials in .env, then either:");
  console.log("  npm run poll   # local poller loop (no lit-triggers needed)");
  console.log("  npm run arm    # register the poller as a lit-triggers cron");
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
      group_name: "price-trigger-stop",
      group_description: "Policy-fenced stop-loss fired by a price poller; price re-checked in-TEE",
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
      name: "priceTriggerStop",
      description: "Stop-loss within policy fences: floor price, max amount, derived-clientOrderId idempotency; reduceOnly on perps",
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
      name: "price-trigger-stop-executor",
      description: "Scoped key used by the price poller (lit-triggers cron or poll.js) to execute the stop action",
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
