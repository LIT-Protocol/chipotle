// One-shot setup for the compliance-transfer-gate example.
//
// What you provide (in .env before running):
//   LIT_API_KEY        Account or usage API key from the dashboard.
//   DEPLOYER_PRIVATE_KEY     EOA private key used to deploy the contract.
//                            Needs gas on whichever network you target.
//
// What this script does, in order:
//   1. Compute the action's IPFS CID via /core/v1/get_lit_action_ipfs_id
//   2. Create a permission group with a wildcard action allowlist
//      (cid_hashes_permitted: ["0"]) via /core/v1/add_group
//   3. Create a scoped usage API key with execute permission in that group
//   4. Derive the action's wallet address by running a one-shot inline
//      Lit Action with the usage key from step 3
//   5. Register the action via /core/v1/add_action (metadata only)
//   6. Wire the specific action CID into the group via
//      /core/v1/add_action_to_group (intent + audit trail)
//   7. Deploy CompliantToken (pinning ACTION_WALLET_ADDRESS as oracle)
//
// Order matters: the deriver in step 4 calls /lit_action, which the
// contract authorizes only when the calling key has an entry in
// usageApiKeys with executeInGroups containing this action's group. So
// the group + usage key must exist before the deriver runs.
//
// Two keys are in play:
//   * LIT_API_KEY     — the ACCOUNT-LEVEL (master) key, used by setup
//                       itself to call management endpoints.
//   * LIT_USAGE_API_KEY — a SCOPED usage key created by step 6 with
//                         execute permission only in this example's
//                         group. transfer.js uses this for /lit_action.
// The master can't execute /lit_action for actions registered in your own
// groups (canExecuteAction inspects usageApiKeys[apiKeyHash]); the scoped
// key is the right shape for that.
//
// Notice what's *not* here: no minted PKP, no encrypted secrets, no API
// keys to provision. The action reads the Chainalysis sanctions oracle
// over a plain public RPC, signs with its CID-derived key, and the
// contract verifies the signature. Pure keyless, end to end.
//
// Every step that produces a new value writes it into .env, so re-runs
// skip whatever's already done. If you edit the action source, step 1
// detects the new CID and clears the now-stale ACTION_WALLET_ADDRESS and
// GROUP_ID so steps 2-5 re-run with the fresh CID.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const env = require("./_env");

const ACTION_FILE = path.join(__dirname, "..", "action", "complianceGate.js");
const DEPLOY_NETWORK = process.env.DEPLOY_NETWORK || "baseSepolia";

// Tiny inline Lit Action that returns the wallet address for any CID.
// Used in step 2 to look up our main action's address from its CID.
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

  // -------------------------------------------------------------------------
  // Step 1: Compute the action's IPFS CID.
  //
  // We pass the action's JS source to the server, which hashes it the
  // same way IPFS would. We do not actually pin the file to IPFS — the
  // network re-derives the CID from inline code at execution time, so
  // registering the CID against your account is enough.
  // -------------------------------------------------------------------------
  const actionCode = fs.readFileSync(ACTION_FILE, "utf8");
  const freshCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, actionCode);
  if (!process.env.ACTION_IPFS_CID || process.env.ACTION_IPFS_CID !== freshCid) {
    if (process.env.ACTION_IPFS_CID) {
      console.log("Step 1/7: action source changed — updating CID...");
      // The old GROUP_ID + usage key were scoped to a now-stale CID; clear
      // them so steps 2-4 recreate a fresh group + key for the new CID.
      env.upsert("ACTION_WALLET_ADDRESS", "");
      env.upsert("GROUP_ID", "");
      env.upsert("LIT_USAGE_API_KEY", "");
    } else {
      console.log("Step 1/7: Computing action CID...");
    }
    env.upsert("ACTION_IPFS_CID", freshCid);
    console.log(`  ACTION_IPFS_CID=${freshCid}`);
  } else {
    console.log(`Step 1/7: action CID unchanged (${freshCid}). Skipping.`);
  }
  const actionCid = process.env.ACTION_IPFS_CID;

  // -------------------------------------------------------------------------
  // Step 2: Create a permission group with a WILDCARD action allowlist.
  //
  // A group is the unit of authorization on Lit: it bundles which action
  // CIDs may execute under which usage API keys. We pass cid_hashes=["0"]
  // which the server parses as U256(0) — the contract treats U256(0) in
  // a group's cidHash set as "any action allowed in this group" (see
  // ViewsFacet.groupIdsForAction: `cidHash.contains(cidHash) || cidHash.contains(0)`).
  // Wildcard is what makes the one-shot deriver action in step 4 below
  // executable — its inline CID isn't pre-registered anywhere.
  //
  // The scoping that still matters: the usage key created in step 3 only
  // has execute permission inside THIS group, so the wildcard is bounded
  // by the usage key's reach.
  // -------------------------------------------------------------------------
  if (!process.env.GROUP_ID) {
    console.log("Step 2/7: Creating group (wildcard action allowlist)...");
    const id = await addGroup(LIT_API_BASE, LIT_API_KEY);
    env.upsert("GROUP_ID", String(id));
    console.log(`  GROUP_ID=${id}`);
  } else {
    console.log(`Step 2/7: group already in .env (${process.env.GROUP_ID}). Skipping.`);
  }
  const groupId = Number(process.env.GROUP_ID);

  // -------------------------------------------------------------------------
  // Step 3: Create a scoped usage API key with execute permission in the
  // group.
  //
  // The contract's canExecuteAction check inspects
  // account.usageApiKeys[apiKeyHash].executeInGroups — which is only
  // populated for keys created via /add_usage_api_key. The master key
  // (LIT_API_KEY) can run management endpoints but can NOT run
  // /lit_action for actions registered in your own groups; that requires
  // a usage key scoped with execute_in_groups: [groupId]. We create one
  // here so step 4 (which calls /lit_action for the deriver) and
  // transfer.js (which calls /lit_action for the compliance action)
  // both have a key the contract authorizes.
  //
  // Heads up: usage keys are shown ONCE by the server. If LIT_USAGE_API_KEY
  // is missing from .env we have to create a fresh one (the old one is
  // unrecoverable).
  // -------------------------------------------------------------------------
  if (!process.env.LIT_USAGE_API_KEY) {
    console.log("Step 3/7: Creating scoped usage API key...");
    const key = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
    env.upsert("LIT_USAGE_API_KEY", key);
    console.log(`  LIT_USAGE_API_KEY=${key.slice(0, 12)}... (full key written to .env)`);
  } else {
    console.log("Step 3/7: usage API key already in .env. Skipping.");
  }
  const usageKey = process.env.LIT_USAGE_API_KEY;

  // -------------------------------------------------------------------------
  // Step 4: Derive the action's wallet address from its CID.
  //
  // Lit.Actions.getLitActionWalletAddress({ ipfsId }) is only callable
  // from inside a Lit Action, so we run a one-shot inline action whose
  // only job is to look up the main action's address. We call this with
  // the scoped usage key (step 3) — its execute permission in the
  // wildcard-action group covers any inline action.
  //
  // The derived address is what the deployed contract will pin as the
  // compliance oracle. Anyone running THIS exact action source will
  // sign with exactly this address; nobody else can.
  // -------------------------------------------------------------------------
  if (!process.env.ACTION_WALLET_ADDRESS) {
    console.log("Step 4/7: Deriving action wallet address from CID...");
    const addr = await deriveActionWalletAddress(LIT_API_BASE, usageKey, actionCid);
    env.upsert("ACTION_WALLET_ADDRESS", addr);
    console.log(`  ACTION_WALLET_ADDRESS=${addr}`);
  } else {
    console.log(
      `Step 4/7: action wallet address already in .env (${process.env.ACTION_WALLET_ADDRESS}). Skipping.`
    );
  }

  // -------------------------------------------------------------------------
  // Step 5: Register the action with the account.
  //
  // Records metadata (name + description) under the account. Strictly
  // optional given the wildcard group authorizes any action, but worth
  // doing for dashboard visibility and as an audit trail.
  // -------------------------------------------------------------------------
  console.log("Step 5/7: Registering action with account...");
  await idempotent(
    () => addAction(LIT_API_BASE, LIT_API_KEY, actionCid),
    "action already registered"
  );

  // -------------------------------------------------------------------------
  // Step 6: Add the specific compliance action CID to the group.
  //
  // Also optional with the wildcard cidHash already in the group, but
  // documents intent and lets you tighten security later by removing the
  // wildcard ("0") entry from the group via update_group, leaving only
  // this CID authorized.
  // -------------------------------------------------------------------------
  console.log("Step 6/7: Adding action to group...");
  await idempotent(
    () => addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid),
    "action already in group"
  );

  // -------------------------------------------------------------------------
  // Step 7: Deploy CompliantToken.
  //
  // The constructor pins ACTION_WALLET_ADDRESS — the address derived
  // from the action's CID — as the compliance oracle. The deploy
  // script writes the resulting contract address back to .env.
  // -------------------------------------------------------------------------
  if (!process.env.COMPLIANT_TOKEN_ADDRESS) {
    console.log(`Step 7/7: Deploying CompliantToken to ${DEPLOY_NETWORK}...`);
    execSync(`npx hardhat run scripts/deploy.js --network ${DEPLOY_NETWORK}`, {
      stdio: "inherit",
      cwd: path.join(__dirname, ".."),
    });
    env.load();
  } else {
    console.log(`Step 7/7: contract already deployed (${process.env.COMPLIANT_TOKEN_ADDRESS}). Skipping.`);
  }

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
  // so the action's payload lives at body.response, not body itself.
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
      // group." Bounded by the scoped usage key (step 3 in setup) — which
      // only has execute permission in THIS group, so the wildcard
      // doesn't grant the holder access to any other group's resources.
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

async function idempotent(fn, label) {
  try {
    await fn();
  } catch (err) {
    const text = (err.message || "").toLowerCase();
    if (
      err.status === 409 ||
      text.includes("already") ||
      text.includes("exists") ||
      text.includes("duplicate")
    ) {
      console.log(`  (${label})`);
      return;
    }
    throw err;
  }
}

main().catch((err) => {
  console.error("\nSetup failed:", err.message);
  if (err.body) console.error("Server said:", err.body);
  process.exit(1);
});
