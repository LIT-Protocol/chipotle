// One-shot setup for the cross-chain-token example.
//
// What you provide (in .env before running):
//   LIT_API_KEY              Account-level (master) Lit API key
//   DEPLOYER_PRIVATE_KEY     EOA with gas on BOTH Base Sepolia and Arb Sepolia
//
// What this script does, in order:
//   1. Compute the action's IPFS CID
//   2. Create a permission group (wildcard action allowlist)
//   3. Create a scoped usage API key with execute_in_groups: [groupId]
//   4. Derive the action's wallet address (uses the usage key)
//   5. Register the action with the account (metadata)
//   6. Add the specific action CID to the group (audit trail)
//   7. Deploy BridgeToken on Base Sepolia (pins ACTION_WALLET_ADDRESS as oracle)
//   8. Deploy BridgeToken on Arbitrum Sepolia (same)
//   9. Wire bridgePartner on each deployment to point at the other side
//
// Re-running does a fresh setup top-to-bottom: every step creates new
// on-chain state and overwrites the corresponding key in .env. The
// previously-minted group / usage key / contracts become orphaned. That's
// fine for an example — production would manage upgrades.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const env = require("./_env");

const ACTION_FILE = path.join(__dirname, "..", "action", "bridgeAction.js");

// Chains the example deploys on. Adding a third would mean: (a) deploy a
// BridgeToken there, (b) add the chain id + hostname regex to RPC_HOSTS in
// the action (which changes the CID and signer address — redeploy
// everything), and (c) extend the wiring loop in step 9.
const CHAINS = [
  {
    network: "baseSepolia",
    chainId: 84532,
    envKey: "BRIDGE_TOKEN_BASE_SEPOLIA",
    label: "Base Sepolia",
  },
  {
    network: "arbitrumSepolia",
    chainId: 421614,
    envKey: "BRIDGE_TOKEN_ARB_SEPOLIA",
    label: "Arbitrum Sepolia",
  },
];

const ADDRESS_DERIVER_CODE = `
  async function main({ ipfsId }) {
    const walletAddress = await Lit.Actions.getLitActionWalletAddress({ ipfsId });
    return { walletAddress };
  }
`;

async function main() {
  env.load();

  const { LIT_API_BASE = "https://api.chipotle.litprotocol.com", LIT_API_KEY } =
    process.env;

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
  console.log("Step 1/9: Computing action CID...");
  const actionCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, actionCode);
  env.upsert("ACTION_IPFS_CID", actionCid);
  console.log(`  ACTION_IPFS_CID=${actionCid}`);

  // -------------------------------------------------------------------------
  // Step 2: Create the group with a wildcard action allowlist.
  // -------------------------------------------------------------------------
  console.log("Step 2/9: Creating group (wildcard action allowlist)...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  // -------------------------------------------------------------------------
  // Step 3: Create a scoped usage API key.
  // -------------------------------------------------------------------------
  console.log("Step 3/9: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(
    `  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`
  );

  // -------------------------------------------------------------------------
  // Step 4: Derive the action's wallet address from its CID.
  // -------------------------------------------------------------------------
  console.log("Step 4/9: Deriving action wallet address from CID...");
  const actionAddr = await deriveActionWalletAddress(
    LIT_API_BASE,
    usageKey,
    actionCid
  );
  env.upsert("ACTION_WALLET_ADDRESS", actionAddr);
  console.log(`  ACTION_WALLET_ADDRESS=${actionAddr}`);

  // -------------------------------------------------------------------------
  // Step 5: Register the action (metadata).
  // -------------------------------------------------------------------------
  console.log("Step 5/9: Registering action with account...");
  await addAction(LIT_API_BASE, LIT_API_KEY, actionCid);

  // -------------------------------------------------------------------------
  // Step 6: Add the specific action CID to the group (audit trail).
  // -------------------------------------------------------------------------
  console.log("Step 6/9: Adding action to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, actionCid);

  // -------------------------------------------------------------------------
  // Steps 7 + 8: Deploy on each chain.
  //
  // On a rerun, the initial env.load() at the top of main() has already
  // populated process.env with the previous run's deployed addresses, and
  // env.load() refuses to overwrite already-set values. Clear them here so
  // the fresh addresses written by deploy.js into .env get picked up by the
  // post-deploy env.load() — otherwise step 9 would wire the new contracts
  // to the OLD addresses and leave the new ones with empty bridgePartner.
  for (const c of CHAINS) delete process.env[c.envKey];
  for (let i = 0; i < CHAINS.length; i++) {
    const c = CHAINS[i];
    console.log(`Step ${7 + i}/9: Deploying BridgeToken on ${c.label}...`);
    execSync(`npx hardhat run scripts/deploy.js --network ${c.network}`, {
      stdio: "inherit",
      cwd: path.join(__dirname, ".."),
    });
    env.load();
  }

  // -------------------------------------------------------------------------
  // Step 9: Wire bridgePartner on each deployment to point at the other.
  //
  // We do this from JS rather than as part of deploy.js because each chain
  // needs to know the OTHER chain's deployed address — which doesn't exist
  // yet during the first deploy.
  // -------------------------------------------------------------------------
  console.log("Step 9/9: Wiring bridge partners on both chains...");
  const { ethers } = require("ethers");
  const abi = [
    "function setBridgePartner(uint256 chainId, address partner) external",
    "function bridgePartner(uint256) view returns (address)",
  ];

  for (const me of CHAINS) {
    const myAddr = process.env[me.envKey];
    const peer = CHAINS.find((c) => c !== me);
    const peerAddr = process.env[peer.envKey];
    if (!myAddr || !peerAddr) {
      throw new Error(
        `missing deployed address: ${me.envKey}=${myAddr}, ${peer.envKey}=${peerAddr}`
      );
    }
    const rpcUrl = rpcForNetwork(me.network);
    const provider = new ethers.providers.JsonRpcProvider(rpcUrl);
    const signer = new ethers.Wallet(
      process.env.DEPLOYER_PRIVATE_KEY,
      provider
    );
    const token = new ethers.Contract(myAddr, abi, signer);
    const tx = await token.setBridgePartner(peer.chainId, peerAddr);
    console.log(
      `  ${me.label}: setBridgePartner(${peer.chainId}, ${peerAddr}) -> ${tx.hash}`
    );
    await tx.wait();
  }

  // -------------------------------------------------------------------------
  console.log("\n✓ Setup complete.\n");
  console.log("  Action CID:            ", process.env.ACTION_IPFS_CID);
  console.log("  Action wallet (oracle):", process.env.ACTION_WALLET_ADDRESS);
  console.log("  Group ID:              ", process.env.GROUP_ID);
  console.log("  Base Sepolia BridgeToken:    ", process.env.BRIDGE_TOKEN_BASE_SEPOLIA);
  console.log("  Arb  Sepolia BridgeToken:    ", process.env.BRIDGE_TOKEN_ARB_SEPOLIA);
  console.log("\nTry it out:");
  console.log("  # Burn on Base Sepolia, mint on Arbitrum Sepolia");
  console.log(
    "  npm run bridge -- --from baseSepolia --to arbitrumSepolia --amount 25 --recipient <addr>"
  );
  console.log("\nFor the reverse direction, swap --from and --to.");
}

function rpcForNetwork(network) {
  if (network === "baseSepolia") {
    return (
      process.env.BASE_SEPOLIA_RPC_URL || "https://sepolia.base.org"
    );
  }
  if (network === "arbitrumSepolia") {
    return (
      process.env.ARBITRUM_SEPOLIA_RPC_URL ||
      "https://sepolia-rollup.arbitrum.io/rpc"
    );
  }
  throw new Error(`unknown network ${network}`);
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
    throw new Error(
      `address derivation failed: ${body.logs || JSON.stringify(body)}`
    );
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
      group_name: "cross-chain-token",
      group_description:
        "Action-derived signer for permissionless burn/mint cross-chain token bridging",
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
      name: "bridgeAction",
      description:
        "Cross-chain burn/mint bridge oracle: reads burn receipts and signs mint authorizations",
    }),
  });
}

async function addActionToGroup(base, apiKey, groupId, cid) {
  return call(base, apiKey, "add_action_to_group", {
    method: "POST",
    body: JSON.stringify({
      group_id: Number(groupId),
      action_ipfs_cid: cid,
    }),
  });
}

async function createUsageApiKey(base, apiKey, groupId) {
  const body = await call(base, apiKey, "add_usage_api_key", {
    method: "POST",
    body: JSON.stringify({
      name: "cross-chain-token-executor",
      description: "Scoped key used by bridge.js to execute the bridge action",
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
    throw new Error(
      `add_usage_api_key returned no key: ${JSON.stringify(body)}`
    );
  }
  return body.usage_api_key;
}

main().catch((err) => {
  console.error("\nSetup failed:", err.message);
  if (err.body) console.error("Server said:", err.body);
  process.exit(1);
});
