// One-shot setup for the prediction-market-oracle example.
//
// What you provide (in .env before running):
//   LIT_API_KEY              Account-level (master) Lit API key
//   PERPLEXITY_API_KEY       Required — web-grounded baseline model
//   OPENAI_API_KEY           Optional — frontier-model second opinion
//   ANTHROPIC_API_KEY        Optional — frontier-model second opinion
//   DEPLOYER_PRIVATE_KEY     EOA used to deploy the registry
//
// Two cryptographic identities at play:
//
//   * The action's derived wallet address (ACTION_WALLET_ADDRESS) —
//     computed from the action's IPFS CID. This is what the deployed
//     PredictionMarket trusts as its `oracle`.
//
//   * The decrypt PKP (DECRYPT_PKP_ADDRESS) — the encryption boundary
//     for the AI provider keys (Encrypt/Decrypt in Lit are PKP-keyed).
//     Signs nothing the market cares about.
//
// What this script does, in order:
//   1. Mint a fresh decrypt PKP
//   2. Compute the action's IPFS CID
//   3. Create a permission group (wildcard action allowlist)
//   4. Authorize the decrypt PKP inside the group
//   5. Create a scoped usage API key with execute_in_groups: [groupId]
//   6. Derive the action's wallet address (uses the usage key)
//   7. Register the action (metadata)
//   8. Add the specific action CID to the group (audit trail)
//   9. Deploy PredictionMarket (pinning ACTION_WALLET_ADDRESS as oracle)
//  10. Encrypt all configured AI provider keys to the decrypt PKP
//
// Re-running this script does a fresh setup top-to-bottom: every step
// creates new on-chain state and overwrites the corresponding key in
// .env. The previously-minted PKP / group / usage key / contract / and
// ciphertexts become orphaned. That's fine for a docs example.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const env = require("./_env");

const ACTION_FILE = path.join(__dirname, "..", "action", "marketOracle.js");
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

  for (const k of [
    "LIT_API_KEY",
    "PERPLEXITY_API_KEY",
    "DEPLOYER_PRIVATE_KEY",
  ]) {
    if (!process.env[k]) {
      throw new Error(
        `${k} is required in .env. Copy .env.example to .env and fill it in.`
      );
    }
  }
  if (!process.env.OPENAI_API_KEY && !process.env.ANTHROPIC_API_KEY) {
    console.log(
      "Note: only Perplexity is configured. Consensus = 1-of-1. " +
        "Set OPENAI_API_KEY and/or ANTHROPIC_API_KEY in .env to require multi-model agreement."
    );
  }

  const actionCode = fs.readFileSync(ACTION_FILE, "utf8");

  // -------------------------------------------------------------------------
  // Step 1: Mint the decrypt PKP.
  // -------------------------------------------------------------------------
  console.log("Step 1/10: Minting decrypt PKP...");
  const { mintPkp } = require("./mintPkp");
  const pkpAddr = await mintPkp();
  console.log(`  DECRYPT_PKP_ADDRESS=${pkpAddr}`);

  // -------------------------------------------------------------------------
  // Step 2: Compute the action's IPFS CID.
  // -------------------------------------------------------------------------
  console.log("Step 2/10: Computing action CID...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, actionCode);
  env.upsert("ACTION_IPFS_CID", actionCid);
  console.log(`  ACTION_IPFS_CID=${actionCid}`);

  // -------------------------------------------------------------------------
  // Step 3: Create the group with a wildcard action allowlist.
  // -------------------------------------------------------------------------
  console.log("Step 3/10: Creating group (wildcard action allowlist)...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  // -------------------------------------------------------------------------
  // Step 4: Authorize the decrypt PKP inside the group.
  //
  // Done BEFORE the deriver in step 6 so everything execution-related is
  // wired up before the first /lit_action call.
  // -------------------------------------------------------------------------
  console.log("Step 4/10: Adding decrypt PKP to group...");
  await addPkpToGroup(LIT_API_BASE, LIT_API_KEY, groupId, pkpAddr);

  // -------------------------------------------------------------------------
  // Step 5: Create a scoped usage API key.
  // -------------------------------------------------------------------------
  console.log("Step 5/10: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(`  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`);

  // -------------------------------------------------------------------------
  // Step 6: Derive the action's wallet address from its CID.
  // -------------------------------------------------------------------------
  console.log("Step 6/10: Deriving action wallet address from CID...");
  const actionAddr = await deriveActionWalletAddress(LIT_API_BASE, usageKey, actionCid);
  env.upsert("ACTION_WALLET_ADDRESS", actionAddr);
  console.log(`  ACTION_WALLET_ADDRESS=${actionAddr}`);

  // -------------------------------------------------------------------------
  // Step 7: Register the action (metadata).
  // -------------------------------------------------------------------------
  console.log("Step 7/10: Registering action with account...");
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);

  // -------------------------------------------------------------------------
  // Step 8: Add the specific action CID to the group (audit trail).
  // -------------------------------------------------------------------------
  console.log("Step 8/10: Adding action to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);

  // -------------------------------------------------------------------------
  // Step 9: Deploy PredictionMarket.
  // -------------------------------------------------------------------------
  console.log(`Step 9/10: Deploying PredictionMarket to ${DEPLOY_NETWORK}...`);
  execSync(`npx hardhat run scripts/deploy.js --network ${DEPLOY_NETWORK}`, {
    stdio: "inherit",
    cwd: path.join(__dirname, ".."),
  });
  env.load();

  // -------------------------------------------------------------------------
  // Step 10: Encrypt all configured AI provider keys to the decrypt PKP.
  // -------------------------------------------------------------------------
  console.log("Step 10/10: Encrypting AI provider keys to decrypt PKP...");
  const { encryptApiKeys } = require("./encryptApiKeys");
  await encryptApiKeys();

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Decrypt PKP:           ", process.env.DECRYPT_PKP_ADDRESS);
  console.log("  Action CID:            ", process.env.ACTION_IPFS_CID);
  console.log("  Action wallet (oracle):", process.env.ACTION_WALLET_ADDRESS);
  console.log("  Group ID:              ", process.env.GROUP_ID);
  console.log("  PredictionMarket:      ", process.env.PREDICTION_MARKET_ADDRESS);

  const configured = ["Perplexity"];
  if (process.env.ENCRYPTED_OPENAI_API_KEY) configured.push("OpenAI");
  if (process.env.ENCRYPTED_ANTHROPIC_API_KEY) configured.push("Anthropic");
  console.log("  AI models in consensus: ", configured.join(", "));

  console.log("\nTry it out:");
  console.log('  npm run propose -- --text "Will 2027 be a leap year?"');
  console.log("  # then, after the resolveAt window (default 5 min)...");
  console.log("  npm run resolve -- --id 0x<the-id-printed-above>");
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
      group_name: "prediction-market-oracle",
      group_description: "AI multi-model consensus for prediction market resolution",
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
      name: "marketOracle",
      description: "Multi-model AI consensus for yes/no prediction market resolution",
    }),
  });
}

async function addActionToGroup(base, apiKey, groupId, cid) {
  return call(base, apiKey, "add_action_to_group", {
    method: "POST",
    body: JSON.stringify({ group_id: Number(groupId), action_ipfs_cid: cid }),
  });
}

async function addPkpToGroup(base, apiKey, groupId, pkpAddress) {
  return call(base, apiKey, "add_pkp_to_group", {
    method: "POST",
    body: JSON.stringify({ group_id: Number(groupId), pkp_id: pkpAddress }),
  });
}

async function createUsageApiKey(base, apiKey, groupId) {
  const body = await call(base, apiKey, "add_usage_api_key", {
    method: "POST",
    body: JSON.stringify({
      name: "prediction-market-oracle-executor",
      description: "Scoped key used by resolve.js + encryptApiKeys.js for /lit_action calls",
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
