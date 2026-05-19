// One-shot setup for the prediction-market-oracle example.
//
// What you provide (in .env before running):
//   LIT_USAGE_API_KEY        Account or usage API key.
//   PERPLEXITY_API_KEY       Required — web-grounded baseline.
//   OPENAI_API_KEY           Optional — frontier-model second opinion.
//   ANTHROPIC_API_KEY        Optional — frontier-model second opinion.
//   DEPLOYER_PRIVATE_KEY     EOA used to deploy the registry.
//
// Two cryptographic identities at play:
//
//   * The action's derived wallet address (ACTION_WALLET_ADDRESS) —
//     computed from the action's IPFS CID. This is what the deployed
//     PredictionMarket trusts as its `oracle`. Editing the action source
//     produces a new CID and therefore a new address; old markets stop
//     trusting the new action.
//
//   * The decrypt PKP (DECRYPT_PKP_ADDRESS) — the encryption boundary for
//     the AI provider keys (Perplexity, optionally OpenAI and Anthropic).
//     It signs nothing the market cares about.
//
// What this script does, in order:
//   1. Mint the decrypt PKP
//   2. Compute the action's IPFS CID
//   3. Derive the action's wallet address from its CID
//   4. Create a permission group
//   5. Register the action
//   6. Authorize the action inside the group
//   7. Authorize the decrypt PKP inside the group
//   8. Deploy PredictionMarket (pinning ACTION_WALLET_ADDRESS as oracle)
//   9. Encrypt all configured AI provider keys to the decrypt PKP

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
    LIT_USAGE_API_KEY,
  } = process.env;

  for (const k of [
    "LIT_USAGE_API_KEY",
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

  // -------------------------------------------------------------------------
  // Step 1: Mint the decrypt PKP.
  // -------------------------------------------------------------------------
  if (!process.env.DECRYPT_PKP_ADDRESS) {
    console.log("Step 1/9: Minting decrypt PKP...");
    const { mintPkp } = require("./mintPkp");
    const addr = await mintPkp();
    console.log(`  DECRYPT_PKP_ADDRESS=${addr}`);
  } else {
    console.log(`Step 1/9: decrypt PKP already in .env (${process.env.DECRYPT_PKP_ADDRESS}). Skipping.`);
  }

  // -------------------------------------------------------------------------
  // Step 2: Compute the action's IPFS CID.
  // -------------------------------------------------------------------------
  const actionCode = fs.readFileSync(ACTION_FILE, "utf8");
  const freshCid = await getActionCid(LIT_API_BASE, LIT_USAGE_API_KEY, actionCode);
  if (!process.env.ACTION_IPFS_CID || process.env.ACTION_IPFS_CID !== freshCid) {
    if (process.env.ACTION_IPFS_CID) {
      console.log("Step 2/9: action source changed — updating CID...");
      env.upsert("ACTION_WALLET_ADDRESS", "");
      env.upsert("GROUP_ID", "");
    } else {
      console.log("Step 2/9: Computing action CID...");
    }
    env.upsert("ACTION_IPFS_CID", freshCid);
    console.log(`  ACTION_IPFS_CID=${freshCid}`);
  } else {
    console.log(`Step 2/9: action CID unchanged (${freshCid}). Skipping.`);
  }
  const actionCid = process.env.ACTION_IPFS_CID;

  // -------------------------------------------------------------------------
  // Step 3: Derive the action's wallet address from its CID.
  // -------------------------------------------------------------------------
  if (!process.env.ACTION_WALLET_ADDRESS) {
    console.log("Step 3/9: Deriving action wallet address from CID...");
    const addr = await deriveActionWalletAddress(
      LIT_API_BASE,
      LIT_USAGE_API_KEY,
      actionCid
    );
    env.upsert("ACTION_WALLET_ADDRESS", addr);
    console.log(`  ACTION_WALLET_ADDRESS=${addr}`);
  } else {
    console.log(
      `Step 3/9: action wallet address already in .env (${process.env.ACTION_WALLET_ADDRESS}). Skipping.`
    );
  }

  // -------------------------------------------------------------------------
  // Step 4: Create a permission group.
  // -------------------------------------------------------------------------
  if (!process.env.GROUP_ID) {
    console.log("Step 4/9: Creating group...");
    const id = await addGroup(LIT_API_BASE, LIT_USAGE_API_KEY);
    env.upsert("GROUP_ID", String(id));
    console.log(`  GROUP_ID=${id}`);
  } else {
    console.log(`Step 4/9: group already in .env (${process.env.GROUP_ID}). Skipping.`);
  }
  const groupId = Number(process.env.GROUP_ID);

  // -------------------------------------------------------------------------
  // Step 5: Register the action.
  // -------------------------------------------------------------------------
  console.log("Step 5/9: Registering action with account...");
  await idempotent(
    () => addAction(LIT_API_BASE, LIT_USAGE_API_KEY, actionCid),
    "action already registered"
  );

  // -------------------------------------------------------------------------
  // Step 6: Authorize the action inside the group.
  // -------------------------------------------------------------------------
  console.log("Step 6/9: Adding action to group...");
  await idempotent(
    () => addActionToGroup(LIT_API_BASE, LIT_USAGE_API_KEY, groupId, actionCid),
    "action already in group"
  );

  // -------------------------------------------------------------------------
  // Step 7: Authorize the decrypt PKP inside the group.
  // -------------------------------------------------------------------------
  console.log("Step 7/9: Adding decrypt PKP to group...");
  await idempotent(
    () =>
      addPkpToGroup(
        LIT_API_BASE,
        LIT_USAGE_API_KEY,
        groupId,
        process.env.DECRYPT_PKP_ADDRESS
      ),
    "pkp already in group"
  );

  // -------------------------------------------------------------------------
  // Step 8: Deploy PredictionMarket.
  // -------------------------------------------------------------------------
  if (!process.env.PREDICTION_MARKET_ADDRESS) {
    console.log(`Step 8/9: Deploying PredictionMarket to ${DEPLOY_NETWORK}...`);
    execSync(`npx hardhat run scripts/deploy.js --network ${DEPLOY_NETWORK}`, {
      stdio: "inherit",
      cwd: path.join(__dirname, ".."),
    });
    env.load();
  } else {
    console.log(`Step 8/9: contract already deployed (${process.env.PREDICTION_MARKET_ADDRESS}). Skipping.`);
  }

  // -------------------------------------------------------------------------
  // Step 9: Encrypt all configured AI provider keys to the decrypt PKP.
  //
  // Always encrypts the *current* plaintexts — re-running setup after
  // adding (or rotating) an optional key will pick it up automatically.
  // -------------------------------------------------------------------------
  console.log("Step 9/9: Encrypting AI provider keys to decrypt PKP...");
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
      group_name: "prediction-market-oracle",
      group_description: "AI multi-model consensus for prediction market resolution",
      pkp_ids_permitted: [],
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
