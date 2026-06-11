// One-shot setup for the cex-sweep-with-email-approval example.
//
// What you provide (in .env before running):
//   LIT_API_KEY   Account-level (master) Lit API key
//
// What this script does, in order:
//   1. Compute the IPFS CIDs of both actions (lit-venues bundle + source)
//   2. Create a permission group (wildcard action allowlist)
//   3. Create a scoped usage API key with execute_in_groups: [groupId]
//   4. Register both actions with the account (metadata)
//   5. Add both action CIDs to the group (audit trail)
//   6. Poll the request action's side-effect-free `probe` branch until the
//      fresh usage key's group grant has propagated (no email is sent)
//
// Re-running this script does a fresh setup top-to-bottom: every step creates
// new state and overwrites the corresponding key in .env. The previously
// minted group / usage key become orphaned. That's fine for a docs example.

const env = require("./_env");
const { runAction, buildCode } = require("./_lit");

const REQUEST_ACTION = "requestSweep.js";
const COMPLETE_ACTION = "completeSweep.js";

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
  console.log("Step 1/6: Computing action CIDs (lit-venues bundle + action source)...");
  const requestCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, buildCode(REQUEST_ACTION));
  const completeCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, buildCode(COMPLETE_ACTION));
  env.upsert("REQUEST_ACTION_CID", requestCid);
  env.upsert("COMPLETE_ACTION_CID", completeCid);
  console.log(`  REQUEST_ACTION_CID=${requestCid}`);
  console.log(`  COMPLETE_ACTION_CID=${completeCid}`);

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
  console.log("Step 4/6: Registering actions with account...");
  await addAction(LIT_API_BASE, LIT_API_KEY, requestCid, {
    name: "cexSweepRequest",
    description:
      "Phase 1: reads Binance spot-testnet balances, applies the sweep policy, requests an L2 email approval",
  });
  await addAction(LIT_API_BASE, LIT_API_KEY, completeCid, {
    name: "cexSweepComplete",
    description:
      "Phase 2: checks the email approval (attestation verified in-TEE) and performs the gated sweep step",
  });

  // -------------------------------------------------------------------------
  console.log("Step 5/6: Adding actions to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, requestCid);
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, completeCid);

  // -------------------------------------------------------------------------
  console.log("Step 6/6: Waiting for the usage key's group grant to propagate...");
  // First use of a brand-new usage key is eventually consistent — poll the
  // request action's `probe` branch (no venue call, no email) with retries
  // rather than letting the first real request-sweep fail on a transient miss.
  const probe = await runAction(
    REQUEST_ACTION,
    { probe: true },
    { retries: 10, delayMs: 3000 }
  );
  if (!probe || !probe.ready) throw new Error("probe did not report ready");
  console.log(`  action runtime ready (lit-venues ${probe.litVenuesVersion})`);

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Request CID: ", process.env.REQUEST_ACTION_CID);
  console.log("  Complete CID:", process.env.COMPLETE_ACTION_CID);
  console.log("  Group ID:    ", process.env.GROUP_ID);
  console.log("\nTry it out:");
  console.log("  npm run request-sweep -- 100 USDT   # phase 1: policy + approval email");
  console.log("  npm run complete-sweep              # phase 2: after the human approves");
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
      group_name: "cex-sweep-with-email-approval",
      group_description:
        "Two-phase CEX sweep gated by an L2 email approval (plan D6)",
      pkp_ids_permitted: [],
      cid_hashes_permitted: ["0"],
    }),
  });
  return body.group_id;
}

async function addAction(base, apiKey, cid, { name, description }) {
  return call(base, apiKey, "add_action", {
    method: "POST",
    body: JSON.stringify({
      action_ipfs_cid: cid,
      name,
      description,
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
      name: "cex-sweep-executor",
      description:
        "Scoped key used by the demo scripts to execute the sweep request/complete actions",
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
