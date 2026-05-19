// One-shot setup for the multi-rpc-consensus-oracle example.
//
// What you provide (in .env before running):
//   LIT_USAGE_API_KEY        Account or usage API key from the dashboard.
//   INFURA_URL               Plaintext Infura RPC URL (with key embedded).
//   ALCHEMY_URL              Plaintext Alchemy RPC URL.
//   QUICKNODE_URL            Plaintext QuickNode RPC URL.
//   DEPLOYER_PRIVATE_KEY     EOA private key used to deploy the registry.
//
// Two cryptographic identities at play:
//
//   * The action's derived wallet address (ACTION_WALLET_ADDRESS) —
//     computed from the action's IPFS CID. This is what the deployed
//     ConsensusOracle trusts as its `signer`. Editing the action source
//     produces a new CID and therefore a new address; old registries stop
//     trusting the new action.
//
//   * The decrypt PKP (DECRYPT_PKP_ADDRESS) — solely the encryption
//     boundary for the three RPC URLs. Encrypt/Decrypt in Lit are
//     PKP-keyed, so we need a PKP just for that. It signs nothing the
//     registry cares about.
//
// What this script does, in order:
//   1. Mint the decrypt PKP via /core/v1/create_wallet
//   2. Compute the action's IPFS CID via /core/v1/get_lit_action_ipfs_id
//   3. Derive the action's wallet address by running a one-shot inline
//      Lit Action that calls Lit.Actions.getLitActionWalletAddress
//   4. Create a permission group via /core/v1/add_group
//   5. Register the action via /core/v1/add_action
//   6. Wire the action into the group via /core/v1/add_action_to_group
//   7. Wire the decrypt PKP into the group via /core/v1/add_pkp_to_group
//   8. Deploy ConsensusOracle (pinning ACTION_WALLET_ADDRESS as signer)
//   9. Encrypt the three RPC URLs to the decrypt PKP
//
// Every step that produces a new value writes it into .env, so re-runs
// skip whatever's already done. If you edit the action source, step 2
// detects the new CID and clears the now-stale ACTION_WALLET_ADDRESS and
// GROUP_ID so steps 3-7 re-run with the fresh CID.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const env = require("./_env");

const ACTION_FILE = path.join(__dirname, "..", "action", "consensusOracle.js");
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
    "INFURA_URL",
    "ALCHEMY_URL",
    "QUICKNODE_URL",
    "DEPLOYER_PRIVATE_KEY",
  ]) {
    if (!process.env[k]) {
      throw new Error(
        `${k} is required in .env. Copy .env.example to .env and fill it in.`
      );
    }
  }

  // -------------------------------------------------------------------------
  // Step 1: Mint the decrypt PKP.
  //
  // This PKP exists only as the encryption boundary for the three RPC
  // URLs. It is *not* what the registry trusts as the signer.
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
  //
  // Lit.Actions.getLitActionWalletAddress is only callable from inside a
  // Lit Action, so we run a one-shot inline action whose only job is to
  // look up the main action's address. The derived address is what the
  // deployed registry will pin as `signer`.
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
  // Step 5: Register the action with the account.
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
  //
  // The action needs the PKP in its group to call Lit.Actions.Decrypt.
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
  // Step 8: Deploy ConsensusOracle.
  //
  // The constructor pins ACTION_WALLET_ADDRESS as `signer`. The deploy
  // script writes the resulting contract address back to .env.
  // -------------------------------------------------------------------------
  if (!process.env.CONSENSUS_ORACLE_ADDRESS) {
    console.log(`Step 8/9: Deploying ConsensusOracle to ${DEPLOY_NETWORK}...`);
    execSync(`npx hardhat run scripts/deploy.js --network ${DEPLOY_NETWORK}`, {
      stdio: "inherit",
      cwd: path.join(__dirname, ".."),
    });
    env.load();
  } else {
    console.log(`Step 8/9: contract already deployed (${process.env.CONSENSUS_ORACLE_ADDRESS}). Skipping.`);
  }

  // -------------------------------------------------------------------------
  // Step 9: Encrypt the three RPC URLs to the decrypt PKP.
  // -------------------------------------------------------------------------
  if (
    !process.env.ENCRYPTED_INFURA_URL ||
    !process.env.ENCRYPTED_ALCHEMY_URL ||
    !process.env.ENCRYPTED_QUICKNODE_URL
  ) {
    console.log("Step 9/9: Encrypting RPC URLs to decrypt PKP...");
    const { encryptRpcUrls } = require("./encryptRpcUrls");
    await encryptRpcUrls();
  } else {
    console.log("Step 9/9: RPC URLs already encrypted. Skipping.");
  }

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Decrypt PKP:           ", process.env.DECRYPT_PKP_ADDRESS);
  console.log("  Action CID:            ", process.env.ACTION_IPFS_CID);
  console.log("  Action wallet (signer):", process.env.ACTION_WALLET_ADDRESS);
  console.log("  Group ID:              ", process.env.GROUP_ID);
  console.log("  ConsensusOracle:       ", process.env.CONSENSUS_ORACLE_ADDRESS);
  console.log("\nTry it out:");
  console.log("  npm run submit -- --token 0x... --holder 0x...");
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

// Run a one-shot inline action that returns the wallet address for a
// given CID. Requires LIT_USAGE_API_KEY to have permission to execute
// arbitrary inline actions (true for account-level keys; for usage keys
// scoped to a group, register this helper's CID against that group too).
async function deriveActionWalletAddress(base, apiKey, cid) {
  const body = await call(base, apiKey, "lit_action", {
    method: "POST",
    body: JSON.stringify({
      code: ADDRESS_DERIVER_CODE,
      js_params: { ipfsId: cid },
    }),
  });
  if (!body.walletAddress) {
    throw new Error(`address derivation returned: ${JSON.stringify(body)}`);
  }
  return body.walletAddress;
}

async function addGroup(base, apiKey) {
  const body = await call(base, apiKey, "add_group", {
    method: "POST",
    body: JSON.stringify({
      group_name: "multi-rpc-consensus-oracle",
      group_description: "Action-derived signer + decrypt PKP for multi-RPC view-function attestations",
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
      name: "consensusOracle",
      description: "Multi-RPC consensus reader for EVM view functions",
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
