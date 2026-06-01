// One-shot setup for the Across testnet integration.
//
// Prereqs in .env (same as the mock demo, plus origin RPC):
//   LIT_API_KEY                Account-level (master) Lit API key
//   ALCHEMY_BASE_SEPOLIA_URL   destination (Base Sepolia) Alchemy URL
//   ALCHEMY_ETH_SEPOLIA_URL    origin (Sepolia) Alchemy URL
//   DEPLOYER_PRIVATE_KEY       EOA with Base-Sepolia gas + a little ETH to wrap
//
// Steps:
//   1. Compute acrossPolicy.js CID
//   2. Create a permission group (wildcard allowlist)
//   3. Create a scoped usage key (ACROSS_USAGE_API_KEY)
//   4. Derive the policy signer address (ACROSS_POLICY_SIGNER_ADDRESS)
//   5. Register the action + add it to the group
//   6. Deploy + fund the AcrossSolverVault (scripts/deploy-across.js)
//
// Re-running does a fresh setup and orphans the previous group/key/vault.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const env = require("./_env");
const lit = require("./_chipotle");

const ACTION_FILE = path.join(__dirname, "..", "action", "acrossPolicy.js");
const DEPLOY_NETWORK = process.env.DEPLOY_NETWORK || "baseSepolia";

async function main() {
  env.load();
  const { LIT_API_BASE = "https://api.chipotle.litprotocol.com", LIT_API_KEY } = process.env;

  for (const k of [
    "LIT_API_KEY",
    "DEPLOYER_PRIVATE_KEY",
    "ALCHEMY_BASE_SEPOLIA_URL",
    "ALCHEMY_ETH_SEPOLIA_URL",
  ]) {
    if (!process.env[k]) {
      throw new Error(`${k} is required in .env. Copy .env.example to .env and fill it in.`);
    }
  }

  const code = fs.readFileSync(ACTION_FILE, "utf8");

  console.log("Step 1/6: Computing acrossPolicy CID...");
  const cid = await lit.getActionCid(LIT_API_BASE, LIT_API_KEY, code);
  env.upsert("ACROSS_ACTION_IPFS_CID", cid);
  console.log(`  ACROSS_ACTION_IPFS_CID=${cid}`);

  console.log("Step 2/6: Creating group...");
  const groupId = await lit.addGroup(
    LIT_API_BASE,
    LIT_API_KEY,
    "lit-solver-vault-across",
    "Policy gate that authorizes Across fills against the on-chain deposit"
  );
  env.upsert("ACROSS_GROUP_ID", String(groupId));
  console.log(`  ACROSS_GROUP_ID=${groupId}`);

  console.log("Step 3/6: Creating scoped usage API key...");
  const usageKey = await lit.createUsageApiKey(
    LIT_API_BASE,
    LIT_API_KEY,
    groupId,
    "lit-solver-vault-across-executor",
    "Scoped key used by the relayer bot to request Across fill authorizations"
  );
  env.upsert("ACROSS_USAGE_API_KEY", usageKey);
  console.log(`  ACROSS_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key written to .env)`);

  console.log("Step 4/6: Deriving policy signer address...");
  const signer = await lit.deriveActionWalletAddress(LIT_API_BASE, usageKey, cid);
  env.upsert("ACROSS_POLICY_SIGNER_ADDRESS", signer);
  console.log(`  ACROSS_POLICY_SIGNER_ADDRESS=${signer}`);

  console.log("Step 5/6: Registering action + adding to group...");
  await lit.addAction(
    LIT_API_BASE,
    LIT_API_KEY,
    cid,
    "acrossPolicy",
    "Authorizes Across relayer fills bound to the on-chain deposit"
  );
  await lit.addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, cid);

  console.log(`Step 6/6: Deploying + funding AcrossSolverVault on ${DEPLOY_NETWORK}...`);
  execSync(`npx hardhat run scripts/deploy-across.js --network ${DEPLOY_NETWORK}`, {
    stdio: "inherit",
    cwd: path.join(__dirname, ".."),
  });
  env.load();

  console.log("\n✓ Across setup complete.\n");
  console.log("  Policy signer:     ", process.env.ACROSS_POLICY_SIGNER_ADDRESS);
  console.log("  AcrossSolverVault: ", process.env.ACROSS_VAULT_ADDRESS);
  console.log("\nWalk the real-integration demo:");
  console.log("  npm run across:deposit   # create a real Across intent on Sepolia");
  console.log("  npm run across:fill      # relayer fills it via the vault on Base Sepolia");
  console.log("  npm run across:attack    # show exfiltration is impossible by construction");
}

main().catch((err) => {
  console.error("\nAcross setup failed:", err.message);
  if (err.body) console.error("Server said:", err.body);
  process.exit(1);
});
