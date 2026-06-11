// One-shot setup for the funding-rate-monitor example.
//
// Provide in .env before running:
//   LIT_API_KEY   Account-level (master) Lit API key
//
// What this script does, in order:
//   1. Compute the monitor's CID (lit-venues bundle + action, hashed together)
//   2. Create a group and pin EXACTLY that CID (no wildcard)
//   3. Mint a scoped usage key with execute_in_groups: [groupId]
//   4. Smoke-run the monitor with an unreachable threshold and no recipient
//      (proves the pipeline end to end; cannot send an email)
//
// Why the pinned CID: this action can call the quota'd sendEmail op, so the
// usage key is scoped to exactly the audited monitor code — a leaked key
// cannot run arbitrary email-sending code against your account quota. The
// flip side: editing the action OR rebuilding lit-venues changes the CID,
// so re-run this script afterwards.
//
// No PKP and no sealed credentials here — both data legs are public.
//
// Re-running does a fresh setup top-to-bottom and overwrites the derived
// values in .env (the previous group / usage key are orphaned).

const env = require("./_env");
const { buildCode, runCode } = require("./_lit");

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

  const code = buildCode(); // throws with build instructions if the bundle is missing

  // -------------------------------------------------------------------------
  console.log("Step 1/4: Computing action CID (bundle + action)...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, code);
  env.upsert("ACTION_IPFS_CID", actionCid);
  console.log(`  ACTION_IPFS_CID=${actionCid}`);

  // -------------------------------------------------------------------------
  console.log("Step 2/4: Creating group and pinning the CID (no wildcard)...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);
  console.log(`  GROUP_ID=${groupId}`);

  // -------------------------------------------------------------------------
  console.log("Step 3/4: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(
    `  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`
  );

  // -------------------------------------------------------------------------
  console.log("Step 4/4: Smoke-running the monitor (no email possible)...");
  // First use of the brand-new usage key — its group grant is eventually
  // consistent, so poll with retries rather than aborting on a transient miss.
  // thresholdPct 1e9 is unreachable and alertTo is omitted: reports only.
  const result = await runCode(
    code,
    { coins: ["BTC"], thresholdPct: 1e9 },
    { retries: 10, delayMs: 3000 }
  );
  const row = result && result.rows && result.rows[0];
  if (!row) throw new Error(`unexpected smoke-run response: ${JSON.stringify(result)}`);
  if (row.hlError) {
    console.log(`  NOTE: hyperliquid leg errored [${row.hlError.code}]: ${row.hlError.message}`);
    console.log("  (often an egress geo-block — see the README's egress note)");
  } else {
    console.log(
      `  BTC funding/hr=${row.fundingHourly} annualized=${row.fundingAnnualizedPct}% ` +
        `mark=${row.hlMark} spot=${row.spotUsd} basis=${row.basisPct}%`
    );
  }

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Action CID:", actionCid);
  console.log("  Group ID:  ", groupId);
  console.log("\nTry it out:");
  console.log("  npm run monitor      # one monitoring pass; emails ALERT_EMAIL on breach");
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
      group_name: "funding-rate-monitor",
      group_description: "Pinned cross-venue funding/basis monitor with sendEmail alerting",
      pkp_ids_permitted: [],
      // No wildcard: the exact CID is pinned via add_action_to_group below,
      // so the usage key can only run the audited monitor code.
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
      name: "funding-rate-monitor",
      description: "Hyperliquid funding vs Coinbase spot basis; sendEmail alert beyond threshold",
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
      name: "funding-rate-monitor-executor",
      description: "Scoped key used by monitor.js to execute the pinned funding monitor",
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
