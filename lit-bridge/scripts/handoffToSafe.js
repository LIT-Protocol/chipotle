// Platform governance handoff: give the BridgeConfigRegistry to the Base Safe.
//
// The registry is the ONLY shared, platform-level contract — it holds the
// per-chain RPC config + quorum that the oracle trusts for EVERY token bridged
// here, so the Safe should govern it. Token contracts are per-issuer (each
// issuer owns their own BridgeToken); they are NOT handed off by this script.
//
// Ownable2Step, so this only PROPOSES the transfer (sets pendingOwner = Safe);
// the Safe then calls acceptOwnership() — propose that with proposeAccepts.js.
// Idempotent: if the registry is already Safe-owned, it's a no-op.
//
// Run as the current registry owner (deployer). Required in .env:
//   SAFE_ADDRESS, DEPLOYER_PRIVATE_KEY, REGISTRY_ADDRESS, ALCHEMY_API_KEY

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
  if (!safe || !ethers.utils.isAddress(safe)) throw new Error("SAFE_ADDRESS missing/invalid");
  for (const k of ["DEPLOYER_PRIVATE_KEY", "REGISTRY_ADDRESS"]) {
    if (!process.env[k]) throw new Error(`${k} missing`);
  }
  const A = process.env.ALCHEMY_API_KEY;
  const base = new ethers.Wallet(
    process.env.DEPLOYER_PRIVATE_KEY,
    new ethers.providers.JsonRpcProvider(`https://base-mainnet.g.alchemy.com/v2/${A}`)
  );
  const registry = new ethers.Contract(process.env.REGISTRY_ADDRESS, abiOf("BridgeConfigRegistry"), base);

  const owner = await registry.owner();
  if (owner.toLowerCase() === safe.toLowerCase()) {
    console.log("Registry is already owned by the Safe — nothing to do.");
    return;
  }
  const pending = await registry.pendingOwner();
  if (pending.toLowerCase() === safe.toLowerCase()) {
    console.log(`Registry transfer already proposed (pendingOwner = Safe). Have the Safe run acceptOwnership() (proposeAccepts.js).`);
    return;
  }

  console.log("Proposing registry ownership -> Safe (two-step; Safe must acceptOwnership)...");
  const tx = await registry.transferOwnership(safe);
  await tx.wait();
  console.log(`  transferOwnership(${safe}) -> ${tx.hash}`);
  console.log("\nNext: the Safe calls acceptOwnership() on the registry (run proposeAccepts.js).");
  console.log("Until accepted, the deployer remains owner (nothing lost if the Safe address was wrong).");
}

main().catch((e) => { console.error("\nhandoffToSafe failed:", e.message); process.exit(1); });
