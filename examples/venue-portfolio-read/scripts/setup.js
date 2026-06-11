// One-shot setup for the venue-portfolio-read example.
//
// Provide in .env before running (see .env.example):
//   LIT_API_KEY                  Account-level (master) Lit API key
//   ...and at least ONE venue:
//   BINANCE_API_KEY + BINANCE_API_SECRET        (read-only key)
//   COINBASE_API_KEY + COINBASE_API_SECRET      (read-only CDP key)
//   HYPERLIQUID_ACCOUNT_ADDRESS                 (no key — reads are public)
//
// What it does, in order:
//   1. Compute two CIDs: the snapshot code (lit-venues bundle + action) and
//      a tiny inline seal-helper action
//   2. Create the vault PKP the credentials are sealed against
//   3. Create a group (NO wildcard), add the PKP, register the snapshot
//      action, pin exactly the two CIDs
//   4. Mint a scoped usage key (execute-only, this group)
//   5. Seal the venue-credentials-v1 JSON against the vault PKP by running
//      the helper in the TEE -> SEALED_VENUE_CREDENTIALS in .env
//
// Why pinned CIDs: the usage key can only execute code in this group, and
// the group permits exactly (a) the audited snapshot code, which decrypts
// and reads balances, and (b) the seal helper, which only encrypts. No other
// code — including a tampered bundle — can ever decrypt the credentials.
// The flip side: editing the action OR rebuilding lit-venues changes the
// snapshot CID, so re-run this script afterwards.
//
// Re-running does a fresh setup top-to-bottom and overwrites the derived
// values in .env (the previous group / usage key / PKP are orphaned).

const env = require("./_env");
const { buildCode, runCode } = require("./_lit");

// Setup-only helper action (inline; its plaintext is auditable right here).
// It can only Encrypt — it has no path to a decrypted secret.
const SEAL_HELPER_CODE = `
  async function main({ pkpId, message }) {
    const ciphertext = await Lit.Actions.Encrypt({ pkpId, message });
    return { ciphertext };
  }
`;

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

  const credentials = buildCredentials();
  const configured = Object.keys(credentials).filter((k) => k !== "v" && k !== "egress");
  if (configured.length === 0) {
    throw new Error(
      "Configure at least one venue in .env: BINANCE_API_KEY+BINANCE_API_SECRET, " +
        "COINBASE_API_KEY+COINBASE_API_SECRET, or HYPERLIQUID_ACCOUNT_ADDRESS."
    );
  }
  console.log(`Venues configured: ${configured.join(", ")}`);

  const snapshotCode = buildCode(); // throws with build instructions if the bundle is missing

  // -------------------------------------------------------------------------
  console.log("Step 1/5: Computing action CIDs (snapshot + seal helper)...");
  const snapshotCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, snapshotCode);
  const helperCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, SEAL_HELPER_CODE);
  env.upsert("ACTION_IPFS_CID", snapshotCid);
  console.log(`  ACTION_IPFS_CID=${snapshotCid} (covers bundle + action together)`);

  // -------------------------------------------------------------------------
  console.log("Step 2/5: Creating vault PKP...");
  const vaultPkp = (await call(LIT_API_BASE, LIT_API_KEY, "create_wallet", { method: "GET" }))
    .wallet_address;
  env.upsert("VAULT_PKP_ADDRESS", vaultPkp);
  console.log(`  VAULT_PKP_ADDRESS=${vaultPkp}`);

  // -------------------------------------------------------------------------
  console.log("Step 3/5: Creating group, adding PKP, pinning the 2 CIDs...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  await addPkpToGroup(LIT_API_BASE, LIT_API_KEY, groupId, vaultPkp);
  await addAction(
    LIT_API_BASE,
    LIT_API_KEY,
    snapshotCid,
    "venue-portfolio-read",
    "Multi-venue balance snapshot; decrypts sealed venue credentials in-TEE (read scope)"
  );
  for (const cid of [snapshotCid, helperCid]) {
    await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, cid);
  }
  console.log(`  GROUP_ID=${groupId} (pinned ${snapshotCid.slice(0, 10)}... + seal helper)`);

  // -------------------------------------------------------------------------
  console.log("Step 4/5: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(
    `  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`
  );

  // -------------------------------------------------------------------------
  console.log("Step 5/5: Sealing venue credentials against the vault PKP...");
  // First use of the brand-new usage key — its group grant is eventually
  // consistent, so poll with retries rather than aborting on a transient miss.
  const sealed = await runCode(
    SEAL_HELPER_CODE,
    { pkpId: vaultPkp, message: JSON.stringify(credentials) },
    { retries: 10, delayMs: 3000 }
  );
  if (!sealed || !sealed.ciphertext) {
    throw new Error(`sealing failed: ${JSON.stringify(sealed)}`);
  }
  env.upsert("SEALED_VENUE_CREDENTIALS", sealed.ciphertext);
  console.log(`  SEALED_VENUE_CREDENTIALS set (${sealed.ciphertext.length} chars)`);

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Vault PKP:  ", vaultPkp);
  console.log("  Action CID: ", snapshotCid);
  console.log("  Group ID:   ", groupId);
  console.log("\nThe runtime path (snapshot.js, cron, CI) only ever needs the usage key");
  console.log("and the ciphertext. You may now remove the plaintext BINANCE_*/COINBASE_*");
  console.log("lines from .env — re-running setup will need them again to re-seal.");
  console.log("\nTry it out:");
  console.log("  npm run snapshot          # attested multi-venue balance snapshot");
}

// ---------------------------------------------------------------------------
// venue-credentials-v1 from .env (only the venues you configured).
// ---------------------------------------------------------------------------

function buildCredentials() {
  const e = process.env;
  const creds = { v: 1 };

  if (e.BINANCE_API_KEY && e.BINANCE_API_SECRET) {
    creds.binance = {
      apiKey: e.BINANCE_API_KEY,
      secret: e.BINANCE_API_SECRET,
      keyType: e.BINANCE_KEY_TYPE === "ed25519" ? "ed25519" : "hmac",
      sandbox: e.BINANCE_SANDBOX === "true", // spot testnet
    };
  }
  if (e.COINBASE_API_KEY && e.COINBASE_API_SECRET) {
    creds.coinbase = {
      apiKey: e.COINBASE_API_KEY,
      // CDP private keys are PEM; .env holds them on one line with literal
      // \n escapes — restore real newlines before sealing.
      secret: e.COINBASE_API_SECRET.replace(/\\n/g, "\n"),
      keyType: "es256-jwt",
    };
  }
  if (e.HYPERLIQUID_ACCOUNT_ADDRESS) {
    creds.hyperliquid = {
      accountAddress: e.HYPERLIQUID_ACCOUNT_ADDRESS,
      sandbox: e.HYPERLIQUID_SANDBOX === "true", // testnet
    };
  }
  if (e.EGRESS_PROXY_URL) {
    // Routed through Lit.Actions.proxiedFetch in-TEE (plan D4). The URL
    // embeds proxy credentials, so it is sealed along with the venue keys.
    creds.egress = { proxyUrl: e.EGRESS_PROXY_URL };
  }
  return creds;
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
      group_name: "venue-portfolio-read",
      group_description: "Vault PKP + pinned snapshot/seal actions for the multi-venue balance read",
      pkp_ids_permitted: [],
      // No wildcard: the exact CIDs are pinned via add_action_to_group, so
      // only that audited code can use the vault PKP (i.e. decrypt the keys).
      cid_hashes_permitted: [],
    }),
  });
  return body.group_id;
}

async function addPkpToGroup(base, apiKey, groupId, pkpId) {
  return call(base, apiKey, "add_pkp_to_group", {
    method: "POST",
    body: JSON.stringify({ group_id: Number(groupId), pkp_id: pkpId }),
  });
}

async function addAction(base, apiKey, cid, name, description) {
  return call(base, apiKey, "add_action", {
    method: "POST",
    body: JSON.stringify({ action_ipfs_cid: cid, name, description }),
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
      name: "venue-portfolio-read-executor",
      description: "Scoped key used by snapshot.js to run the pinned portfolio actions",
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
