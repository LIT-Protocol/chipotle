// One-shot setup for the compliance-transfer-gate example.
//
// What you provide (in .env before running):
//   LIT_API_KEY              Account-level (master) Lit API key
//   DEPLOYER_PRIVATE_KEY     EOA used to deploy the contract
//   SCREENING_RPC_URL        eth-mainnet.g.alchemy.com URL with your key
//
// Two cryptographic identities at play:
//
//   * The action's derived wallet address (ACTION_WALLET_ADDRESS) —
//     computed from the action's IPFS CID. This is what the deployed
//     CompliantToken trusts as its compliance oracle. Editing the action
//     source produces a new CID and therefore a new address.
//
//   * LIT_USAGE_API_KEY — a scoped usage key with execute permission
//     inside this example's group. transfer.js uses it; the master
//     LIT_API_KEY can't execute /lit_action for actions in your own
//     groups (the contract's canExecuteAction only consults
//     usageApiKeys[apiKeyHash]).
//
// What this script does, in order:
//   1. Compute the action's IPFS CID
//   2. Create a permission group with a wildcard action allowlist
//      (cid_hashes_permitted: ["0"] → U256(0) in the group's cidHash set
//      means "any action allowed here," needed for inline helper actions
//      like the deriver in step 4)
//   3. Create a scoped usage API key with execute_in_groups: [groupId]
//   4. Derive the action's wallet address by running a one-shot inline
//      Lit Action with the usage key from step 3
//   5. Register the action with the account (metadata)
//   6. Add the specific action CID to the group (audit trail)
//   7. Deploy CompliantToken (pinning ACTION_WALLET_ADDRESS as oracle)
//
// Re-running this script does a fresh setup top-to-bottom: every step
// creates new on-chain state and overwrites the corresponding key in
// .env. The previously-minted group / usage key / contract become
// orphaned. That's fine for a docs example — a production app would
// reuse them, but we're optimizing for "explain what every endpoint
// does," not for cost or convenience.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const env = require("./_env");

const ACTION_FILE = path.join(__dirname, "..", "action", "complianceGate.js");
const DEPLOY_NETWORK = process.env.DEPLOY_NETWORK || "baseSepolia";

// Tiny inline Lit Action that returns the wallet address for any CID.
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

  const actionCode = fs.readFileSync(ACTION_FILE, "utf8");

  // -------------------------------------------------------------------------
  // Step 1: Compute the action's IPFS CID.
  // -------------------------------------------------------------------------
  console.log("Step 1/7: Computing action CID...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, actionCode);
  env.upsert("ACTION_IPFS_CID", actionCid);
  console.log(`  ACTION_IPFS_CID=${actionCid}`);

  // -------------------------------------------------------------------------
  // Step 2: Create the group with a wildcard action allowlist.
  // -------------------------------------------------------------------------
  console.log("Step 2/7: Creating group (wildcard action allowlist)...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  // -------------------------------------------------------------------------
  // Step 3: Create a scoped usage API key.
  // -------------------------------------------------------------------------
  console.log("Step 3/7: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(`  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`);

  // -------------------------------------------------------------------------
  // Step 4: Derive the action's wallet address from its CID.
  // -------------------------------------------------------------------------
  console.log("Step 4/7: Deriving action wallet address from CID...");
  const actionAddr = await deriveActionWalletAddress(LIT_API_BASE, usageKey, actionCid);
  env.upsert("ACTION_WALLET_ADDRESS", actionAddr);
  console.log(`  ACTION_WALLET_ADDRESS=${actionAddr}`);

  // -------------------------------------------------------------------------
  // Step 5: Register the action with the account (metadata).
  // -------------------------------------------------------------------------
  console.log("Step 5/7: Registering action with account...");
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);

  // -------------------------------------------------------------------------
  // Step 6: Add the specific action CID to the group (audit trail).
  // -------------------------------------------------------------------------
  console.log("Step 6/7: Adding action to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);

  // -------------------------------------------------------------------------
  // Step 7: Deploy CompliantToken.
  // -------------------------------------------------------------------------
  console.log(`Step 7/7: Deploying CompliantToken to ${DEPLOY_NETWORK}...`);
  execSync(`npx hardhat run scripts/deploy.js --network ${DEPLOY_NETWORK}`, {
    stdio: "inherit",
    cwd: path.join(__dirname, ".."),
  });
  env.load();

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Action CID:            ", process.env.ACTION_IPFS_CID);
  console.log("  Action wallet (oracle):", process.env.ACTION_WALLET_ADDRESS);
  console.log("  Group ID:              ", process.env.GROUP_ID);
  console.log("  CompliantToken:        ", process.env.COMPLIANT_TOKEN_ADDRESS);
  console.log("\nTry it out:");
  console.log("  npm run transfer -- --to 0xRecipient --amount 100");
  console.log("\nTo see the gate reject, try:");
  console.log("  npm run transfer -- --to 0x7F367cC41522cE07553e823bf3be79A889DEbe1B --amount 100");
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
      group_name: "compliance-transfer-gate",
      group_description: "Action-derived signer for on-chain-Chainalysis transfer gating",
      pkp_ids_permitted: [],
      // U256(0) in the group's cidHash set = "any action allowed in this
      // group." Bounded by the scoped usage key (step 3), which only has
      // execute permission inside THIS group.
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
      name: "complianceGate",
      description: "Sanctions screening via on-chain Chainalysis oracle",
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
      name: "compliance-transfer-gate-executor",
      description: "Scoped key used by transfer.js to execute the gate action",
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
