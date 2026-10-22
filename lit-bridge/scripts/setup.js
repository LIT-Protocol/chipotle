// One-shot, resumable setup for lit-bridge on Base Sepolia + Arbitrum Sepolia.
//
// Required in lit-bridge/.env (see .env.example):
//   LIT_API_KEY, DEPLOYER_PRIVATE_KEY, ALCHEMY_API_KEY, INFURA_API_KEY
//
// What it does (each step writes its result to .env and is skipped on re-run):
//   1.  forge build  (compile contracts)
//   2.  Deploy BridgeConfigRegistry on Base Sepolia (owner = REGISTRY_OWNER || deployer)
//   3.  Create the dedicated signing account (PKP) — Option B
//   4.  Build the action with REGISTRY_ADDRESS + BRIDGE_PKP_ID injected; compute CIDs
//   5.  Create group, add PKP, pin the 3 CIDs, mint a scoped usage key
//   6.  Derive the oracle address (the key the action signs mints with)
//   7.  Encrypt ALCHEMY_API_KEY + INFURA_API_KEY against the PKP (in-TEE)
//   8.  Populate the registry: setChain(chain, minConf, quorum=2, [alchemy, infura]) for both chains
//   9.  Deploy BridgeToken on both chains (oracle pinned); mint supply on the home chain
//   10. Wire bridgePartner both directions (write-once)

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const { ethers } = require("ethers");
const env = require("./_env");
const lit = require("./lit");
const { buildAction } = require("./buildAction");

const CONTRACTS_DIR = path.join(__dirname, "..", "contracts");

function loadArtifact(name) {
  const p = path.join(CONTRACTS_DIR, "out", `${name}.sol`, `${name}.json`);
  const j = JSON.parse(fs.readFileSync(p, "utf8"));
  return { abi: j.abi, bytecode: j.bytecode.object };
}

async function main() {
  env.load();
  const { LIT_API_BASE = "https://api.chipotle.litprotocol.com", LIT_API_KEY } = process.env;
  for (const k of ["LIT_API_KEY", "DEPLOYER_PRIVATE_KEY", "ALCHEMY_API_KEY", "INFURA_API_KEY"]) {
    if (!process.env[k]) throw new Error(`${k} is required in lit-bridge/.env`);
  }

  const minConf = Number(process.env.MIN_CONFIRMATIONS || 5);
  const quorum = Number(process.env.CHAIN_QUORUM || 2);
  const homeNetwork = process.env.INITIAL_SUPPLY_NETWORK || "base";
  const initialSupply = ethers.utils.parseUnits(process.env.INITIAL_SUPPLY || "1000000", 18);

  // Broadcast via Alchemy (reliable for deploys); the action's verification reads
  // go through the encrypted Alchemy/Infura keys with M-of-N consensus.
  const A = process.env.ALCHEMY_API_KEY;
  const baseProvider = new ethers.providers.JsonRpcProvider(`https://base-mainnet.g.alchemy.com/v2/${A}`);
  const arbProvider = new ethers.providers.JsonRpcProvider(`https://arb-mainnet.g.alchemy.com/v2/${A}`);
  const baseSigner = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, baseProvider);
  const arbSigner = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, arbProvider);

  const CHAINS = [
    { id: 8453, name: "Base", network: "base", signer: baseSigner, tokenEnv: "BRIDGE_TOKEN_BASE_MAINNET" },
    { id: 42161, name: "Arbitrum", network: "arbitrum", signer: arbSigner, tokenEnv: "BRIDGE_TOKEN_ARB_MAINNET" },
  ];

  // 1. Compile -------------------------------------------------------------
  console.log("Step 1/10: forge build...");
  execSync("forge build", { cwd: CONTRACTS_DIR, stdio: "inherit" });
  const registryArt = loadArtifact("BridgeConfigRegistry");
  const tokenArt = loadArtifact("BridgeToken");

  // 2. Deploy registry on Base Sepolia ------------------------------------
  if (!process.env.REGISTRY_ADDRESS) {
    console.log("Step 2/10: Deploying BridgeConfigRegistry on Base mainnet...");
    const owner = process.env.REGISTRY_OWNER || (await baseSigner.getAddress());
    const factory = new ethers.ContractFactory(registryArt.abi, registryArt.bytecode, baseSigner);
    const c = await factory.deploy(owner);
    await c.deployed();
    env.upsert("REGISTRY_ADDRESS", c.address);
    console.log(`  REGISTRY_ADDRESS=${c.address}  owner=${owner}`);
  } else {
    console.log(`Step 2/10: registry exists (${process.env.REGISTRY_ADDRESS}) — skip`);
  }

  // 3. Create the signing account (PKP) -----------------------------------
  if (!process.env.BRIDGE_PKP_ID) {
    console.log("Step 3/10: Creating dedicated signing account (PKP)...");
    const pkp = await lit.createWallet(LIT_API_BASE, LIT_API_KEY);
    env.upsert("BRIDGE_PKP_ID", pkp);
    console.log(`  BRIDGE_PKP_ID=${pkp}`);
  } else {
    console.log(`Step 3/10: signing account exists (${process.env.BRIDGE_PKP_ID}) — skip`);
  }

  // 4. Build action + CIDs -------------------------------------------------
  console.log("Step 4/10: Building action (injecting registry + pkp) and computing CIDs...");
  const builtCode = buildAction(process.env.REGISTRY_ADDRESS, process.env.BRIDGE_PKP_ID);
  const bridgeCid = await lit.getActionCid(LIT_API_BASE, LIT_API_KEY, builtCode);
  const encryptCid = await lit.getActionCid(LIT_API_BASE, LIT_API_KEY, lit.ENCRYPT_SECRET_CODE);
  const deriverCid = await lit.getActionCid(LIT_API_BASE, LIT_API_KEY, lit.SIGNER_DERIVER_CODE);
  env.upsert("ACTION_IPFS_CID", bridgeCid);
  console.log(`  ACTION_IPFS_CID=${bridgeCid}`);

  // 5. Group + PKP + pin CIDs + usage key ---------------------------------
  // Group/PKP/usage-key are created once. The CID pinning runs EVERY time so
  // an action upgrade (new CID) is re-pinned — re-running setup after editing
  // the action is the upgrade path. Pinning is idempotent; ignore "already".
  const ignoreExists = async (p) => {
    try { await p; } catch (e) {
      if (!/exist|already|duplicate/i.test(e.message || "")) throw e;
    }
  };
  if (!process.env.GROUP_ID || !process.env.LIT_USAGE_API_KEY) {
    console.log("Step 5/10: Creating group, adding PKP, minting usage key...");
    const groupId = await lit.addGroup(
      LIT_API_BASE, LIT_API_KEY, "lit-bridge",
      "Bridge signing account + pinned actions (oracle, encrypt, deriver)"
    );
    env.upsert("GROUP_ID", String(groupId));
    await lit.addPkpToGroup(LIT_API_BASE, LIT_API_KEY, groupId, process.env.BRIDGE_PKP_ID);
    const usageKey = await lit.createUsageApiKey(
      LIT_API_BASE, LIT_API_KEY, groupId, "lit-bridge-executor",
      "Scoped key the relayer / bridge.js uses to run the bridge action"
    );
    env.upsert("LIT_USAGE_API_KEY", usageKey);
    console.log(`  GROUP_ID=${groupId}  usage key=${usageKey.slice(0, 12)}...`);
  } else {
    console.log(`Step 5/10: group ${process.env.GROUP_ID} exists — re-pinning current CIDs`);
  }
  const groupId = Number(process.env.GROUP_ID);
  await ignoreExists(lit.addAction(LIT_API_BASE, LIT_API_KEY, bridgeCid, "bridgeAction",
    "lit-bridge oracle: M-of-N consensus over burns -> signs/relays mint"));
  for (const cid of [bridgeCid, encryptCid, deriverCid]) {
    await ignoreExists(lit.addActionToGroup(LIT_API_BASE, LIT_API_KEY, groupId, cid));
  }
  console.log(`  pinned CID ${bridgeCid} (+ encrypt, deriver) to group ${groupId}`);
  const usageKey = process.env.LIT_USAGE_API_KEY;

  // 6. Derive oracle address ----------------------------------------------
  if (!process.env.ORACLE_ADDRESS) {
    console.log("Step 6/10: Deriving oracle address (the key the action signs with)...");
    const r = await lit.runAction(LIT_API_BASE, usageKey, lit.SIGNER_DERIVER_CODE, {
      pkpId: process.env.BRIDGE_PKP_ID,
    });
    if (!r || !r.address) throw new Error(`oracle derivation failed: ${JSON.stringify(r)}`);
    env.upsert("ORACLE_ADDRESS", r.address);
    console.log(`  ORACLE_ADDRESS=${r.address}`);
  } else {
    console.log(`Step 6/10: oracle ${process.env.ORACLE_ADDRESS} — skip`);
  }

  // 7. Encrypt provider keys ----------------------------------------------
  if (!process.env.ENC_ALCHEMY || !process.env.ENC_INFURA) {
    console.log("Step 7/10: Encrypting Alchemy + Infura keys against the PKP...");
    const encA = await lit.runAction(LIT_API_BASE, usageKey, lit.ENCRYPT_SECRET_CODE, {
      pkpId: process.env.BRIDGE_PKP_ID, message: process.env.ALCHEMY_API_KEY,
    });
    const encI = await lit.runAction(LIT_API_BASE, usageKey, lit.ENCRYPT_SECRET_CODE, {
      pkpId: process.env.BRIDGE_PKP_ID, message: process.env.INFURA_API_KEY,
    });
    if (!encA?.ciphertext || !encI?.ciphertext) {
      throw new Error("key encryption failed");
    }
    env.upsert("ENC_ALCHEMY", encA.ciphertext);
    env.upsert("ENC_INFURA", encI.ciphertext);
    console.log(`  encrypted (alchemy ${encA.ciphertext.length} chars, infura ${encI.ciphertext.length} chars)`);
  } else {
    console.log("Step 7/10: provider keys already encrypted — skip");
  }

  // 8. Populate registry ---------------------------------------------------
  if (!process.env.REGISTRY_POPULATED) {
    console.log(`Step 8/10: Writing chain config (quorum ${quorum}, minConf ${minConf}) for both chains...`);
    const registry = new ethers.Contract(process.env.REGISTRY_ADDRESS, registryArt.abi, baseSigner);
    // RpcType: 0=Alchemy, 1=Infura. Hostnames are built in the action from
    // code-resident maps; only the encrypted key is stored.
    const rpcs = [
      [0, "", process.env.ENC_ALCHEMY],
      [1, "", process.env.ENC_INFURA],
    ];
    for (const c of CHAINS) {
      const tx = await registry.setChain(c.id, minConf, quorum, rpcs);
      await tx.wait();
      console.log(`  setChain(${c.id}) -> ${tx.hash}`);
    }
    env.upsert("REGISTRY_POPULATED", "1");
  } else {
    console.log("Step 8/10: registry already populated — skip");
  }

  // 9. Deploy tokens -------------------------------------------------------
  for (const c of CHAINS) {
    if (process.env[c.tokenEnv]) {
      console.log(`Step 9/10: ${c.name} token exists (${process.env[c.tokenEnv]}) — skip`);
      continue;
    }
    console.log(`Step 9/10: Deploying BridgeToken on ${c.name}...`);
    const supply = c.network === homeNetwork ? initialSupply : ethers.constants.Zero;
    const factory = new ethers.ContractFactory(tokenArt.abi, tokenArt.bytecode, c.signer);
    const t = await factory.deploy(
      process.env.TOKEN_NAME || "Bridge Coin",
      process.env.TOKEN_SYMBOL || "BRDG",
      supply,
      process.env.ORACLE_ADDRESS
    );
    await t.deployed();
    env.upsert(c.tokenEnv, t.address);
    console.log(`  ${c.tokenEnv}=${t.address}  supply=${supply.toString()}`);
  }

  // 10. Wire partners (write-once) ----------------------------------------
  console.log("Step 10/10: Wiring bridge partners (write-once)...");
  for (const me of CHAINS) {
    const peer = CHAINS.find((c) => c !== me);
    const token = new ethers.Contract(
      process.env[me.tokenEnv], tokenArt.abi, me.signer
    );
    const existing = await token.bridgePartner(peer.id);
    if (existing && existing !== ethers.constants.AddressZero) {
      console.log(`  ${me.name}: partner for ${peer.id} already set (${existing}) — skip`);
      continue;
    }
    const tx = await token.setBridgePartner(peer.id, process.env[peer.tokenEnv]);
    await tx.wait();
    console.log(`  ${me.name}: setBridgePartner(${peer.id}, ${process.env[peer.tokenEnv]}) -> ${tx.hash}`);
  }

  // 11. Fee config (skim -> treasury) -------------------------------------
  // Idempotent: only sends a tx when the on-chain config differs from .env.
  const feeTreasury = process.env.FEE_TREASURY || (await baseSigner.getAddress());
  const feeFlat = ethers.utils.parseUnits(process.env.FEE_FLAT || "0", 18);
  const feeBps = Number(process.env.FEE_BPS || 10); // default 0.1%
  console.log(`Step 11/11: Setting fee config (treasury=${feeTreasury}, flat=${process.env.FEE_FLAT || "0"}, bps=${feeBps})...`);
  for (const c of CHAINS) {
    const token = new ethers.Contract(process.env[c.tokenEnv], tokenArt.abi, c.signer);
    const [curT, curBps, curFlat] = await Promise.all([token.feeTreasury(), token.feeBps(), token.feeFlat()]);
    if (curT.toLowerCase() === feeTreasury.toLowerCase() && Number(curBps) === feeBps && curFlat.eq(feeFlat)) {
      console.log(`  ${c.name}: fee config already current — skip`);
      continue;
    }
    const tx = await token.setFeeConfig(feeTreasury, feeFlat, feeBps);
    await tx.wait();
    console.log(`  ${c.name}: setFeeConfig -> ${tx.hash}`);
  }

  console.log("\n✓ Setup complete.");
  console.log("  Registry:      ", process.env.REGISTRY_ADDRESS, "(Base mainnet)");
  console.log("  Oracle (signer):", process.env.ORACLE_ADDRESS);
  console.log("  Signing PKP:   ", process.env.BRIDGE_PKP_ID);
  console.log("  Action CID:    ", process.env.ACTION_IPFS_CID);
  console.log("  Token (Base):  ", process.env.BRIDGE_TOKEN_BASE_MAINNET);
  console.log("  Token (Arb):   ", process.env.BRIDGE_TOKEN_ARB_MAINNET);
  console.log("\nNext: fund the PKP (fundPkp.js), then registerTriggers.js, then burn.js");
}

main().catch((err) => {
  console.error("\nSetup failed:", err.message);
  if (err.body) console.error("Server said:", JSON.stringify(err.body));
  process.exit(1);
});
