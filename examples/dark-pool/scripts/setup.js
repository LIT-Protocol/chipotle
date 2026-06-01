// One-shot setup for the dark-pool example.
//
// Provide in .env before running:
//   LIT_API_KEY            Account-level (master) Lit API key
//   DATABASE_URL           Neon connection string (HTTP SQL capable)
//   DEPLOYER_PRIVATE_KEY   EOA to deploy the contracts (optional — if unset,
//                          the Lit-side setup still completes and you can
//                          exercise the encrypt/match actions; only on-chain
//                          settlement needs the contracts)
//
// What it does, in order:
//   1. Compute the encryptOrder + matchEpoch action CIDs
//   2. Create the vault PKP (encrypts orders AND the DB connection string)
//   3. Create a group + add the PKP + mint a scoped usage key
//   4. Encrypt DATABASE_URL against the vault PKP -> ENCRYPTED_DATABASE_URL
//   5. Create the DB schema (direct from here; setup is trusted and already
//      holds the raw URL)
//   6. Derive the matchEpoch action's wallet address (the contract's matcher)
//   7. Register both actions with the account + add them to the group (audit)
//   8. If DEPLOYER_PRIVATE_KEY is set: deploy TestTokens + DarkPoolSettlement
//
// Re-running does a fresh setup top-to-bottom and overwrites the derived
// values in .env (the previous group / usage key / contracts are orphaned).
// Optimised for "explain every endpoint", not for cost — same stance as the
// sibling examples.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const env = require("./_env");

const ENCRYPT_ACTION_FILE = path.join(__dirname, "..", "action", "encryptOrder.js");
const MATCH_ACTION_FILE = path.join(__dirname, "..", "action", "matchEpoch.js");
const MARK_ACTION_FILE = path.join(__dirname, "..", "action", "markSettled.js");

// Tiny setup-only helper actions (inline; their plaintext is auditable here).
const ENCRYPT_SECRET_CODE = `
  async function main({ pkpId, message }) {
    const ciphertext = await Lit.Actions.Encrypt({ pkpId, message });
    return { ciphertext };
  }
`;
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
    DATABASE_URL,
  } = process.env;

  for (const k of ["LIT_API_KEY", "DATABASE_URL"]) {
    if (!process.env[k]) {
      throw new Error(`${k} is required in .env. Copy .env.example to .env and fill it in.`);
    }
  }

  const encryptCode = fs.readFileSync(ENCRYPT_ACTION_FILE, "utf8");
  const matchCode = fs.readFileSync(MATCH_ACTION_FILE, "utf8");
  const markCode = fs.readFileSync(MARK_ACTION_FILE, "utf8");

  // 1. CIDs ------------------------------------------------------------------
  // Compute CIDs for the two runtime actions AND the two setup-only helpers.
  // The group pins exactly these CIDs — no wildcard — so the scoped usage key
  // can only ever run audited code against the vault PKP. A new/modified action
  // has a different CID and is rejected. (Only matchEpoch decrypts orders;
  // encryptOrder/markSettled decrypt only the DB url; encryptSecret only
  // encrypts; the deriver doesn't touch the PKP.)
  console.log("Step 1/7: Computing action CIDs (3 runtime + 2 setup helpers)...");
  const encryptCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, encryptCode);
  const matchCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, matchCode);
  const markCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, markCode);
  const secretCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, ENCRYPT_SECRET_CODE);
  const deriverCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, ADDRESS_DERIVER_CODE);
  env.upsert("ENCRYPT_ACTION_CID", encryptCid);
  env.upsert("MATCH_ACTION_CID", matchCid);
  console.log(`  ENCRYPT_ACTION_CID=${encryptCid}`);
  console.log(`  MATCH_ACTION_CID=${matchCid}`);

  // 2. Vault PKP -------------------------------------------------------------
  console.log("Step 2/7: Creating vault PKP...");
  const vaultPkp = (await call(LIT_API_BASE, LIT_API_KEY, "create_wallet", { method: "GET" }))
    .wallet_address;
  env.upsert("VAULT_PKP_ADDRESS", vaultPkp);
  console.log(`  VAULT_PKP_ADDRESS=${vaultPkp}`);

  // 3. Group + PKP + pinned actions + usage key ------------------------------
  console.log("Step 3/7: Creating group, adding PKP, pinning the 5 CIDs, minting usage key...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  await addPkpToGroup(LIT_API_BASE, LIT_API_KEY, groupId, vaultPkp);
  // Register + pin all CIDs (server hashes the CID for the group allowlist).
  await addAction(LIT_API_BASE, LIT_API_KEY, encryptCid, "encryptOrder", "Seal + store a dark-pool order");
  await addAction(LIT_API_BASE, LIT_API_KEY, matchCid, "matchEpoch", "Match an epoch + sign settlement");
  await addAction(LIT_API_BASE, LIT_API_KEY, markCid, "markSettled", "Mark an epoch's orders settled");
  for (const cid of [encryptCid, matchCid, markCid, secretCid, deriverCid]) {
    await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, cid);
  }
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(`  GROUP_ID=${groupId}  pinned 5 CIDs  usage key=${usageKey.slice(0, 12)}...`);

  // 4. Encrypt the DB url against the vault PKP ------------------------------
  console.log("Step 4/7: Encrypting DATABASE_URL against the vault PKP...");
  const encUrl = await runAction(LIT_API_BASE, usageKey, ENCRYPT_SECRET_CODE, {
    pkpId: vaultPkp,
    message: DATABASE_URL,
  });
  if (!encUrl || !encUrl.ciphertext) throw new Error(`url encryption failed: ${JSON.stringify(encUrl)}`);
  env.upsert("ENCRYPTED_DATABASE_URL", encUrl.ciphertext);
  console.log(`  ENCRYPTED_DATABASE_URL set (${encUrl.ciphertext.length} chars)`);

  // 5. Schema ----------------------------------------------------------------
  console.log("Step 5/7: Creating DB schema on Neon...");
  await createSchema(DATABASE_URL);
  console.log("  schema applied (orders, epochs)");

  // 6. Matcher address -------------------------------------------------------
  console.log("Step 6/7: Deriving matchEpoch wallet address (the contract's matcher)...");
  const derived = await runAction(LIT_API_BASE, usageKey, ADDRESS_DERIVER_CODE, { ipfsId: matchCid });
  if (!derived || !derived.walletAddress) throw new Error(`address derivation failed: ${JSON.stringify(derived)}`);
  env.upsert("MATCH_ACTION_ADDRESS", derived.walletAddress);
  console.log(`  MATCH_ACTION_ADDRESS=${derived.walletAddress}`);

  // 7. Contracts (optional) --------------------------------------------------
  if (process.env.DEPLOYER_PRIVATE_KEY) {
    const network = process.env.DEPLOY_NETWORK || "baseSepolia";
    console.log(`Step 7/7: Deploying contracts to ${network}...`);
    execSync(`npx hardhat run scripts/deploy.js --network ${network}`, {
      stdio: "inherit",
      cwd: path.join(__dirname, ".."),
    });
    // deploy.js wrote the addresses to .env from a child process; env.load()
    // won't clobber values already in our process.env (so CLI overrides win),
    // so re-read the deploy outputs from the file directly for the summary.
    const envText = fs.readFileSync(path.join(__dirname, "..", ".env"), "utf8");
    for (const k of ["BASE_TOKEN_ADDRESS", "QUOTE_TOKEN_ADDRESS", "SETTLEMENT_ADDRESS"]) {
      const m = envText.match(new RegExp(`^${k}=(.*)$`, "m"));
      if (m) process.env[k] = m[1].trim();
    }
  } else {
    console.log("Step 7/7: SKIPPED (no DEPLOYER_PRIVATE_KEY). Lit-side setup is complete.");
    console.log("          Set DEPLOYER_PRIVATE_KEY and re-run to deploy contracts.");
  }

  console.log("\n✓ Setup complete.");
  console.log("  Vault PKP:        ", process.env.VAULT_PKP_ADDRESS);
  console.log("  Matcher (action): ", process.env.MATCH_ACTION_ADDRESS);
  console.log("  Group:            ", process.env.GROUP_ID);
  if (process.env.SETTLEMENT_ADDRESS) {
    console.log("  Settlement:       ", process.env.SETTLEMENT_ADDRESS);
    console.log("  Base / Quote:     ", process.env.BASE_TOKEN_ADDRESS, "/", process.env.QUOTE_TOKEN_ADDRESS);
  }
}

// ---------------------------------------------------------------------------
// Lit Chipotle REST helpers
// ---------------------------------------------------------------------------
async function call(base, apiKey, p, init = {}) {
  const res = await fetch(`${base}/core/v1/${p}`, {
    ...init,
    headers: { "X-Api-Key": apiKey, "Content-Type": "application/json", ...(init.headers || {}) },
  });
  const body = await res.json();
  if (!res.ok) {
    const msg = body.message || body.error || JSON.stringify(body);
    const err = new Error(`${p} -> ${res.status}: ${msg}`);
    err.body = body;
    throw err;
  }
  return body;
}

async function getActionCid(base, apiKey, code) {
  return call(base, apiKey, "get_lit_action_ipfs_id", { method: "POST", body: JSON.stringify(code) });
}

async function runAction(base, apiKey, code, jsParams) {
  const body = await call(base, apiKey, "lit_action", {
    method: "POST",
    body: JSON.stringify({ code, js_params: jsParams }),
  });
  if (body.has_error) throw new Error(`action error: ${body.logs || JSON.stringify(body)}`);
  return body.response;
}

async function addGroup(base, apiKey) {
  const body = await call(base, apiKey, "add_group", {
    method: "POST",
    body: JSON.stringify({
      group_name: "dark-pool",
      group_description: "Vault PKP + pinned actions for the confidential batch auction",
      pkp_ids_permitted: [],
      // No wildcard. The specific CIDs are pinned via add_action_to_group in
      // step 3, so only those exact audited actions can use the vault PKP. A
      // modified or unknown action has a different CID and is rejected — that's
      // what stops even the usage-key holder from swapping in a decrypt action.
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

async function createUsageApiKey(base, apiKey, groupId) {
  const body = await call(base, apiKey, "add_usage_api_key", {
    method: "POST",
    body: JSON.stringify({
      name: "dark-pool-executor",
      description: "Scoped key used by submitOrder.js / runEpoch.js",
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

// Direct Neon HTTP SQL (setup is local + trusted and already holds the raw URL).
async function createSchema(dbUrl) {
  const sql = fs.readFileSync(path.join(__dirname, "..", "schema.sql"), "utf8");
  const stmts = sql
    .split("\n")
    .filter((l) => !l.trim().startsWith("--"))
    .join("\n")
    .split(";")
    .map((s) => s.trim())
    .filter(Boolean);
  const host = new URL(dbUrl).host;
  for (const stmt of stmts) {
    const res = await fetch(`https://${host}/sql`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Neon-Connection-String": dbUrl,
        "Neon-Array-Mode": "true",
      },
      body: JSON.stringify({ query: stmt, params: [] }),
    });
    if (!res.ok) throw new Error(`schema stmt failed (${res.status}): ${await res.text()}`);
  }
}

main().catch((err) => {
  console.error("\nSetup failed:", err.message);
  if (err.body) console.error("Server said:", err.body);
  process.exit(1);
});
