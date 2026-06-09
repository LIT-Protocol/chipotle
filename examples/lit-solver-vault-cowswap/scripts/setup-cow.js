// One-shot setup for the self-contained CoW Protocol integration.
//
// Prereqs in .env:
//   LIT_API_KEY                Account-level (master) Lit API key
//   ALCHEMY_BASE_SEPOLIA_URL   Base-Sepolia Alchemy URL (host-whitelisted by the action)
//   DEPLOYER_PRIVATE_KEY       EOA with Base-Sepolia gas (deploys + becomes vault owner)
//
// Steps:
//   1. Compute cowPolicy.js CID
//   2. Create a permission group (wildcard allowlist)
//   3. Create a scoped usage key (COW_USAGE_API_KEY)
//   4. Derive the policy signer address (COW_POLICY_SIGNER_ADDRESS)
//   5. Register the action + add it to the group
//   6. Deploy the CoW stack + vault (scripts/deploy-cow.js)
//
// Re-running does a fresh setup and orphans the previous group/key/contracts.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const env = require("./_env");
const lit = require("./_chipotle");

const ACTION_FILE = path.join(__dirname, "..", "action", "cowPolicy.js");

async function main() {
  env.load();
  const { LIT_API_BASE = "https://api.chipotle.litprotocol.com", LIT_API_KEY } = process.env;

  for (const k of ["LIT_API_KEY", "DEPLOYER_PRIVATE_KEY", "ALCHEMY_BASE_SEPOLIA_URL"]) {
    if (!process.env[k]) {
      throw new Error(`${k} is required in .env. Copy .env.example to .env and fill it in.`);
    }
  }

  const code = fs.readFileSync(ACTION_FILE, "utf8");

  console.log("Step 1/6: Computing cowPolicy CID...");
  const cid = await lit.getActionCid(LIT_API_BASE, LIT_API_KEY, code);
  env.upsert("COW_ACTION_IPFS_CID", cid);
  console.log(`  COW_ACTION_IPFS_CID=${cid}`);

  console.log("Step 2/6: Creating group...");
  const groupId = await lit.addGroup(
    LIT_API_BASE,
    LIT_API_KEY,
    "lit-solver-vault-cowswap",
    "Policy gate that authorizes CoW settlements built from the trader's signed order"
  );
  env.upsert("COW_GROUP_ID", String(groupId));
  console.log(`  COW_GROUP_ID=${groupId}`);

  console.log("Step 3/6: Creating scoped usage API key...");
  const usageKey = await lit.createUsageApiKey(
    LIT_API_BASE,
    LIT_API_KEY,
    groupId,
    "lit-solver-vault-cowswap-executor",
    "Scoped key the solver bot uses to request CoW settlement authorizations"
  );
  env.upsert("COW_USAGE_API_KEY", usageKey);
  console.log(`  COW_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`);

  console.log("Step 4/6: Deriving policy signer address...");
  const signer = await lit.deriveActionWalletAddress(LIT_API_BASE, usageKey, cid);
  env.upsert("COW_POLICY_SIGNER_ADDRESS", signer);
  console.log(`  COW_POLICY_SIGNER_ADDRESS=${signer}`);

  console.log("Step 5/6: Registering action + adding to group...");
  await lit.addAction(
    LIT_API_BASE,
    LIT_API_KEY,
    cid,
    "cowPolicy",
    "Builds + authorizes a CoW settlement bound to the trader's signed order"
  );
  await lit.addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, cid);

  console.log("Step 6/6: Deploying CoW stack + vault on Base Sepolia...");
  execSync("npx hardhat run scripts/deploy-cow.js --network baseSepolia", {
    stdio: "inherit",
    cwd: path.join(__dirname, ".."),
  });
  env.load(true); // force-reload: the child deploy wrote fresh addresses to .env

  console.log("\n✓ CoW setup complete.\n");
  console.log("  Policy signer:   ", process.env.COW_POLICY_SIGNER_ADDRESS);
  console.log("  GPv2Settlement:  ", process.env.COW_SETTLEMENT_ADDRESS);
  console.log("  CowSolverVault:  ", process.env.COW_VAULT_ADDRESS);
  console.log("\nWalk the demo:");
  console.log("  npm run order    # the trader signs an order (the intent) + approves the relayer");
  console.log("  npm run solve    # the bot asks Lit to authorize, the vault settles it");
  console.log("  npm run attack   # show a compromised bot can't self-deal");
}

main().catch((err) => {
  console.error("\nCoW setup failed:", err.message);
  if (err.body) console.error("Server said:", err.body);
  process.exit(1);
});
