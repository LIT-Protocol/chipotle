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
];

const ARTIFACTS_DIR = "artifacts/contracts";
const OUT_NAME = "AccountConfig";

async function findArtifact(name) {
  // Hardhat stores artifacts at artifacts/contracts/<File>.sol/<Name>.json
  const pattern = `${ARTIFACTS_DIR}/**/${name}.json`;
  const matches = [];
  for await (const f of glob(pattern)) {
    // Skip debug artifacts (*.dbg.json)
    if (!f.endsWith(".dbg.json")) matches.push(f);
  }
  if (matches.length === 0) throw new Error(`Artifact not found for: ${name}`);
  if (matches.length > 1) {
    console.warn(`Multiple artifacts for ${name}: ${matches.join(", ")} — using first`);
  }
  return matches[0];
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

// Write to the same place hardhat-diamond-abi would have written it.
// Preserve bytecode from the compiled AccountConfig.sol so the deployer can deploy it.
const outDir = path.join(ARTIFACTS_DIR, `${OUT_NAME}.sol`);
fs.mkdirSync(outDir, { recursive: true });
const outPath = path.join(outDir, `${OUT_NAME}.json`);
let compiled = {};
try {
  compiled = JSON.parse(fs.readFileSync(outPath, "utf8"));
} catch {
  console.warn(`No pre-existing ${OUT_NAME}.json; run "npx hardhat compile" first.`);
}
// Preserve constructor from compiled AccountConfig.sol (needed for deployment)
const constructorItem = (compiled.abi ?? []).find((item) => item.type === "constructor");
const abi = constructorItem ? [constructorItem, ...combined] : combined;

const output = {
  ...compiled,
  contractName: OUT_NAME,
  abi,
};
fs.writeFileSync(outPath, JSON.stringify(output, null, 2));
console.log(`\nDiamond ABI written → ${outPath} (${combined.length} total entries)`);
