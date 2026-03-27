/**
 * Combines ABI fragments from multiple facet contracts into a single
 * AccountConfig.json artifact — replacing the hardhat-diamond-abi plugin
 * which only supports Hardhat v2.
 *
 * Usage: node generate-diamond-abi.mjs
 * Run after: npx hardhat compile
 */

import fs from "fs";
import path from "path";
import { glob } from "fs/promises";

const FACETS = [
  'WritesFacet',
  'ViewsFacet',
  'APIConfigFacet',
  'BillingFacet',
  'OwnershipFacet',
];

const ARTIFACTS_DIR = "artifacts/contracts";
// Pre-built artifacts for facets not compiled by Hardhat (e.g. OwnershipFacet)
const DIAMOND_ARTIFACTS_DIR = "../rust_generator_and_deployer/src/diamond";
const OUT_NAME = "AccountConfig";

async function findArtifact(name) {
  // Hardhat stores artifacts at artifacts/contracts/<File>.sol/<Name>.json
  for (const dir of [ARTIFACTS_DIR, "artifacts"]) {
    const pattern = `${dir}/**/${name}.json`;
    for await (const f of glob(pattern)) {
      if (!f.endsWith(".dbg.json")) return f;
    }
  }
  // Fallback: pre-built artifacts in the Rust deployer (e.g. OwnershipFacet)
  const fallback = path.join(DIAMOND_ARTIFACTS_DIR, `${name}.json`);
  if (fs.existsSync(fallback)) return fallback;
  throw new Error(`Artifact not found for: ${name}`);
}

function sigKey(item) {
  if (item.type === "function" || item.type === "error") {
    const inputs = (item.inputs ?? []).map((i) => i.type).join(",");
    return `${item.type}:${item.name}(${inputs})`;
  }
  if (item.type === "event") {
    const inputs = (item.inputs ?? []).map((i) => i.type).join(",");
    return `event:${item.name}(${inputs})`;
  }
  return `${item.type}:${item.name ?? ""}`;
}

const combined = [];
const seen = new Set();

for (const facet of FACETS) {
  const artifactPath = await findArtifact(facet);
  const artifact = JSON.parse(fs.readFileSync(artifactPath, "utf8"));
  for (const item of artifact.abi ?? []) {
    const key = sigKey(item);
    if (!seen.has(key)) {
      seen.add(key);
      combined.push(item);
    }
  }
  console.log(`  + ${facet} (${artifact.abi?.length ?? 0} entries from ${artifactPath})`);
}

// Write to the same place hardhat-diamond-abi would have written it
const outDir = path.join(ARTIFACTS_DIR, `${OUT_NAME}.sol`);
fs.mkdirSync(outDir, { recursive: true });
const outPath = path.join(outDir, `${OUT_NAME}.json`);
fs.writeFileSync(
  outPath,
  JSON.stringify({ contractName: OUT_NAME, abi: combined }, null, 2),
);
console.log(`\nDiamond ABI written → ${outPath} (${combined.length} total entries)`);
