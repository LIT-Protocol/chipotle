// One-shot setup for the private-stablecoin example.
//
// What you provide (in .env before running):
//   LIT_API_KEY               Account-level (master) Lit API key
//   DEPLOYER_PRIVATE_KEY      EOA with Base Sepolia gas (deploys + operates)
//   SCREENING_RPC_URL         eth-mainnet.g.alchemy.com URL (OFAC oracle)
//   KYC_SIGNER_PRIVATE_KEY    demo stand-in for a KYC provider's signing key
//
// Cryptographic identities at play:
//   * LEDGER_PKP_ADDRESS    — encrypt/decrypt key for note contents. Minted
//                             here, authorized inside the group so the ledger
//                             and disclose actions can use it.
//   * ACTION_WALLET_ADDRESS — the ledger action's CID-derived signer. The
//                             PrivUSD contract pins this as its sole authority.
//   * KYC_SIGNER_ADDRESS    — derived from KYC_SIGNER_PRIVATE_KEY; the mint
//                             flow verifies attestations against it.
//   * LIT_USAGE_API_KEY     — scoped key the demo uses for /lit_action calls.
//
// Re-running does a fresh setup top-to-bottom and orphans the previous
// group / key / PKP / contracts. Fine for a docs example.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const { ethers } = require("ethers");
const env = require("./_env");
const { buildActions } = require("./lib/buildAction");

const DEPLOY_NETWORK = process.env.DEPLOY_NETWORK || "baseSepolia";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const ADDRESS_DERIVER_CODE = `
  async function main({ ipfsId }) {
    const walletAddress = await Lit.Actions.getLitActionWalletAddress({ ipfsId });
    return { walletAddress };
  }
`;

// Poll the EXACT dependency until it works, rather than guessing a sleep. We
// run the real (registered) ledger action with op:"ping" — which just Encrypts
// with the ledger PKP — because the binding propagation is this action's
// add_action_to_group authorization, not the PKP-to-group one (an inline probe
// would pass while the registered action still can't use the PKP).
async function waitForPkpUsable(base, usageKey, ledgerCode, { tries = 40, intervalMs = 3000 } = {}) {
  for (let i = 0; i < tries; i++) {
    try {
      const body = await call(base, usageKey, "lit_action", {
        method: "POST",
        body: JSON.stringify({ code: ledgerCode, js_params: { op: "ping" } }),
      });
      if (!body.has_error && body.response && body.response.ok) {
        console.log(`  ledger action can use the PKP after ~${Math.round((i * intervalMs) / 1000)}s`);
        return;
      }
    } catch {
      // transient API error — keep polling
    }
    await sleep(intervalMs);
  }
  throw new Error("ledger action never became able to use the PKP (timed out)");
}

async function main() {
  env.load();

  const { LIT_API_BASE = "https://api.chipotle.litprotocol.com", LIT_API_KEY } = process.env;

  for (const k of ["LIT_API_KEY", "DEPLOYER_PRIVATE_KEY", "KYC_SIGNER_PRIVATE_KEY"]) {
    if (!process.env[k]) {
      throw new Error(`${k} is required in .env. Copy .env.example to .env and fill it in.`);
    }
  }

  // Step 1: Derive the KYC signer's public address (no network call).
  console.log("Step 1/11: Deriving KYC signer address...");
  const kycSigner = new ethers.Wallet(process.env.KYC_SIGNER_PRIVATE_KEY).address;
  env.upsert("KYC_SIGNER_ADDRESS", kycSigner);
  console.log(`  KYC_SIGNER_ADDRESS=${kycSigner}`);

  // Step 2: Mint the ledger PKP (encrypt/decrypt boundary for notes).
  console.log("Step 2/11: Minting ledger PKP...");
  const { mintPkp } = require("./mintPkp");
  const pkpAddr = await mintPkp();
  console.log(`  LEDGER_PKP_ADDRESS=${pkpAddr}`);

  // Bake the freshly-minted PKP into both actions' source so it's bound into
  // their CIDs (a caller can't redirect encryption to another PKP).
  const { ledgerCode, discloseCode } = buildActions(pkpAddr);

  // Step 3: Compute both action CIDs.
  console.log("Step 3/11: Computing action CIDs...");
  const ledgerCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, ledgerCode);
  const discloseCid = await getActionCid(LIT_API_BASE, LIT_API_KEY, discloseCode);
  env.upsert("ACTION_IPFS_CID", ledgerCid);
  env.upsert("DISCLOSE_ACTION_IPFS_CID", discloseCid);
  console.log(`  ACTION_IPFS_CID=${ledgerCid}`);
  console.log(`  DISCLOSE_ACTION_IPFS_CID=${discloseCid}`);

  // Step 4: Create the group (wildcard action allowlist).
  console.log("Step 4/11: Creating group (wildcard action allowlist)...");
  const groupId = await addGroup(LIT_API_BASE, LIT_API_KEY);
  env.upsert("GROUP_ID", String(groupId));
  console.log(`  GROUP_ID=${groupId}`);

  // Step 5: Authorize the ledger PKP inside the group (before any execution).
  console.log("Step 5/11: Adding ledger PKP to group...");
  await addPkpToGroup(LIT_API_BASE, LIT_API_KEY, groupId, pkpAddr);

  // Step 6: Create a scoped usage API key.
  console.log("Step 6/11: Creating scoped usage API key...");
  const usageKey = await createUsageApiKey(LIT_API_BASE, LIT_API_KEY, groupId);
  env.upsert("LIT_USAGE_API_KEY", usageKey);
  console.log(`  LIT_USAGE_API_KEY=${usageKey.slice(0, 12)}... (full key in .env)`);

  // Step 7: Derive the ledger action's wallet address (the contract authority).
  console.log("Step 7/11: Deriving ledger action wallet address...");
  const actionAddr = await deriveActionWalletAddress(LIT_API_BASE, usageKey, ledgerCid);
  env.upsert("ACTION_WALLET_ADDRESS", actionAddr);
  console.log(`  ACTION_WALLET_ADDRESS=${actionAddr}`);

  // Step 8: Register both actions (metadata).
  console.log("Step 8/11: Registering actions with account...");
  await addAction(LIT_API_BASE, LIT_API_KEY, ledgerCid, "privUSD-ledger", "mint/transfer/redeem prover");
  await addAction(LIT_API_BASE, LIT_API_KEY, discloseCid, "privUSD-disclose", "warrant-gated note disclosure");

  // Step 9: Add both action CIDs to the group (audit trail).
  console.log("Step 9/11: Adding actions to group...");
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, ledgerCid);
  await addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, discloseCid);

  // Group membership (the PKP + action CIDs) is written on-chain, and the
  // execution node reads it via a Base RPC that lags the sequencer. The lag is
  // variable (it tracks when the underlying tx confirms), so a fixed sleep is
  // unreliable. Instead poll the exact dependency: run a throwaway action that
  // Encrypts with the ledger PKP using the scoped usage key, and wait until it
  // succeeds. Until then a real call fails "API key cannot use selected wallet
  // in selected action".
  console.log("Step 9.5/11: Waiting for the ledger action to be able to use the PKP...");
  await waitForPkpUsable(LIT_API_BASE, usageKey, ledgerCode);

  // Step 10: Deploy MockUSDC + PrivUSD.
  console.log(`Step 10/11: Deploying contracts to ${DEPLOY_NETWORK}...`);
  execSync(`npx hardhat run scripts/deploy.js --network ${DEPLOY_NETWORK}`, {
    stdio: "inherit",
    cwd: path.join(__dirname, ".."),
  });
  env.load();

  // Step 11: Done.
  console.log("\n✓ Setup complete.\n");
  console.log("  Ledger PKP:            ", process.env.LEDGER_PKP_ADDRESS);
  console.log("  Ledger action CID:     ", process.env.ACTION_IPFS_CID);
  console.log("  Ledger oracle (signer):", process.env.ACTION_WALLET_ADDRESS);
  console.log("  Disclose action CID:   ", process.env.DISCLOSE_ACTION_IPFS_CID);
  console.log("  KYC signer:            ", process.env.KYC_SIGNER_ADDRESS);
  console.log("  Group ID:              ", process.env.GROUP_ID);
  console.log("  MockUSDC:              ", process.env.MOCK_USDC_ADDRESS);
  console.log("  PrivUSD:               ", process.env.PRIVUSD_ADDRESS);
  console.log("\nRun the demo:");
  console.log("  npm run demo");
}

// ---------------------------------------------------------------------------
// Lit Chipotle REST helpers.
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

async function deriveActionWalletAddress(base, apiKey, cid) {
  const body = await call(base, apiKey, "lit_action", {
    method: "POST",
    body: JSON.stringify({ code: ADDRESS_DERIVER_CODE, js_params: { ipfsId: cid } }),
  });
  if (body.has_error) throw new Error(`address derivation failed: ${body.logs || JSON.stringify(body)}`);
  if (!body.response || !body.response.walletAddress) {
    throw new Error(`address derivation returned: ${JSON.stringify(body)}`);
  }
  return body.response.walletAddress;
}

async function addGroup(base, apiKey) {
  const body = await call(base, apiKey, "add_group", {
    method: "POST",
    body: JSON.stringify({
      group_name: "private-stablecoin",
      group_description: "privUSD ledger + disclosure actions and the note-encryption PKP",
      pkp_ids_permitted: [],
      cid_hashes_permitted: ["0"], // wildcard: any action allowed in this group
    }),
  });
  return body.group_id;
}

async function addPkpToGroup(base, apiKey, groupId, pkpAddress) {
  return call(base, apiKey, "add_pkp_to_group", {
    method: "POST",
    body: JSON.stringify({ group_id: Number(groupId), pkp_id: pkpAddress }),
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
      name: "private-stablecoin-executor",
      description: "Scoped key used by demo.js / disclose.js for /lit_action calls",
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

main().catch((err) => {
  console.error("\nSetup failed:", err.message);
  if (err.body) console.error("Server said:", err.body);
  process.exit(1);
});
