// Phase 6 (contract side): hand on-chain ownership to the Base Safe.
//
// Transfers BridgeConfigRegistry + both BridgeTokens to SAFE_ADDRESS and points
// each token's fee treasury at the Safe. Run as the CURRENT owner (deployer)
// AFTER the production setup. All three contracts are Ownable2Step, so this only
// PROPOSES the transfer — the Safe must then call acceptOwnership() on each
// (a Safe tx the signers execute). Nothing is irreversible until the Safe accepts.
//
// Required in .env: SAFE_ADDRESS, DEPLOYER_PRIVATE_KEY (current owner),
//   REGISTRY_ADDRESS, BRIDGE_TOKEN_BASE_MAINNET, BRIDGE_TOKEN_ARB_MAINNET, ALCHEMY_API_KEY

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");

function abiOf(name) {
  const p = path.join(__dirname, "..", "contracts", "out", `${name}.sol`, `${name}.json`);
  return JSON.parse(fs.readFileSync(p, "utf8")).abi;
}

async function main() {
  env.load();
  const safe = process.env.SAFE_ADDRESS;
  if (!safe || !ethers.utils.isAddress(safe)) {
    throw new Error("SAFE_ADDRESS missing/invalid — set the Base Safe address in .env");
  }
  for (const k of ["DEPLOYER_PRIVATE_KEY", "REGISTRY_ADDRESS", "BRIDGE_TOKEN_BASE_MAINNET", "BRIDGE_TOKEN_ARB_MAINNET"]) {
    if (!process.env[k]) throw new Error(`${k} missing — run the production setup first`);
  }
  const A = process.env.ALCHEMY_API_KEY;
  const feeFlat = ethers.utils.parseUnits(process.env.FEE_FLAT || "0", 18);
  const feeBps = Number(process.env.FEE_BPS || 10);

  const base = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, new ethers.providers.JsonRpcProvider(`https://base-mainnet.g.alchemy.com/v2/${A}`));
  const arb = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, new ethers.providers.JsonRpcProvider(`https://arb-mainnet.g.alchemy.com/v2/${A}`));

  const registry = new ethers.Contract(process.env.REGISTRY_ADDRESS, abiOf("BridgeConfigRegistry"), base);
  const baseTok = new ethers.Contract(process.env.BRIDGE_TOKEN_BASE_MAINNET, abiOf("BridgeToken"), base);
  const arbTok = new ethers.Contract(process.env.BRIDGE_TOKEN_ARB_MAINNET, abiOf("BridgeToken"), arb);

  // 1. Point each token's fee treasury at the Safe (while we still own them).
  console.log("Setting fee treasury -> Safe on both tokens...");
  await (await baseTok.setFeeConfig(safe, feeFlat, feeBps)).wait();
  await (await arbTok.setFeeConfig(safe, feeFlat, feeBps)).wait();

  // 2. Propose ownership transfer to the Safe (two-step: Safe must accept).
  console.log("Proposing ownership -> Safe (two-step; Safe must acceptOwnership)...");
  const r1 = await (await registry.transferOwnership(safe)).wait();
  const r2 = await (await baseTok.transferOwnership(safe)).wait();
  const r3 = await (await arbTok.transferOwnership(safe)).wait();
  console.log(`  registry (Base):   transferOwnership -> ${r1.transactionHash}`);
  console.log(`  token (Base):      transferOwnership -> ${r2.transactionHash}`);
  console.log(`  token (Arbitrum):  transferOwnership -> ${r3.transactionHash}`);

  console.log("\n⚠ ACTION REQUIRED — the Safe must accept ownership (one Safe tx each):");
  console.log(`  Base:      acceptOwnership() on ${process.env.REGISTRY_ADDRESS} (registry) and ${process.env.BRIDGE_TOKEN_BASE_MAINNET} (token)`);
  console.log(`  Arbitrum:  acceptOwnership() on ${process.env.BRIDGE_TOKEN_ARB_MAINNET} (token)`);
  console.log("Until accepted, the deployer remains owner (nothing is lost if a Safe address was wrong).");
}

main().catch((e) => { console.error("\nhandoffToSafe failed:", e.message); process.exit(1); });
