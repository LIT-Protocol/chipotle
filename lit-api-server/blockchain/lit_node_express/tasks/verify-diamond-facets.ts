import { task } from "hardhat/config";
import { readFileSync } from "fs";
import path from "path";

// Facet artifacts to verify against (same set deployed by contract_deployer).
const FACET_ARTIFACTS = [
  "artifacts/contracts/AccountConfigFacets/APIConfigFacet.sol/APIConfigFacet.json",
  "artifacts/contracts/AccountConfigFacets/BillingFacet.sol/BillingFacet.json",
  "artifacts/contracts/AccountConfigFacets/ViewsFacet.sol/ViewsFacet.json",
  "artifacts/contracts/AccountConfigFacets/WritesFacet.sol/WritesFacet.json",
];

/**
 * Strip the CBOR-encoded metadata suffix from EVM bytecode so that bytecodes
 * compiled on different machines (different source paths, compiler metadata
 * hashes) can still be compared meaningfully.
 *
 * The last two bytes of Solidity-compiled bytecode encode the length of the
 * CBOR metadata block. We read that length and chop off the suffix.
 */
function stripMetadata(bytecode: string): string {
  const bc = bytecode.startsWith("0x") ? bytecode.slice(2) : bytecode;
  if (bc.length < 4) return bc;
  const metadataLength = parseInt(bc.slice(-4), 16);
  // metadata length + the 2-byte length field itself (4 hex chars)
  const suffixHexLen = (metadataLength + 2) * 2;
  if (suffixHexLen >= bc.length) return bc; // sanity: don't strip everything
  return bc.slice(0, bc.length - suffixHexLen);
}

task(
  "verify-diamond-facets",
  "Verify on-chain diamond facets match compiled artifacts"
)
  .addParam("diamond", "Diamond proxy contract address")
  .setAction(async (taskArgs, hre) => {
    const { diamond: diamondAddress } = taskArgs;

    console.log(`Network: ${hre.network.name}`);
    console.log(`Diamond: ${diamondAddress}`);

    const provider = new hre.ethers.JsonRpcProvider(
      (hre.network.config as { url?: string }).url
    );

    // --- 1. Load compiled artifacts and extract selectors per facet ---

    interface ArtifactInfo {
      name: string;
      selectors: Set<string>;
      deployedBytecode: string;
    }

    const artifacts: ArtifactInfo[] = [];
    for (const relPath of FACET_ARTIFACTS) {
      const absPath = path.resolve(relPath);
      const artifact = JSON.parse(readFileSync(absPath, "utf-8"));
      const iface = new hre.ethers.Interface(artifact.abi);
      const selectors = new Set<string>();
      for (const fn of iface.fragments) {
        if (fn.type === "function") {
          selectors.add(iface.getFunction(fn.name)!.selector);
        }
      }
      const name = path.basename(relPath, ".json");
      artifacts.push({
        name,
        selectors,
        deployedBytecode: artifact.deployedBytecode,
      });
    }

    // --- 2. Query on-chain diamond for facets via DiamondLoupe ---

    const loupeAbi = [
      "function facets() view returns (tuple(address facetAddress, bytes4[] functionSelectors)[])",
      "function facetAddresses() view returns (address[])",
      "function facetFunctionSelectors(address) view returns (bytes4[])",
    ];
    const loupe = new hre.ethers.Contract(diamondAddress, loupeAbi, provider);

    const onChainFacets: { facetAddress: string; functionSelectors: string[] }[] =
      await loupe.facets();

    console.log(
      `\nOn-chain diamond has ${onChainFacets.length} facets with ${onChainFacets.reduce((n, f) => n + f.functionSelectors.length, 0)} selectors total.\n`
    );

    // --- 3. Match on-chain facets to compiled artifacts by selectors ---

    let allMatch = true;

    for (const artifact of artifacts) {
      // Find the on-chain facet whose selectors overlap with this artifact
      const matchingFacet = onChainFacets.find((f) =>
        f.functionSelectors.some((sel) =>
          artifact.selectors.has(sel.slice(0, 10))
        )
      );

      if (!matchingFacet) {
        console.log(`FAIL  ${artifact.name}: no matching on-chain facet found`);
        allMatch = false;
        continue;
      }

      // Compare selector sets
      const onChainSelectors = new Set(
        matchingFacet.functionSelectors.map((s) => s.slice(0, 10))
      );
      const missingOnChain = [...artifact.selectors].filter(
        (s) => !onChainSelectors.has(s)
      );
      const extraOnChain = [...onChainSelectors].filter(
        (s) => !artifact.selectors.has(s)
      );

      if (missingOnChain.length > 0 || extraOnChain.length > 0) {
        console.log(
          `FAIL  ${artifact.name} @ ${matchingFacet.facetAddress}: selector mismatch`
        );
        if (missingOnChain.length)
          console.log(`      missing on-chain: ${missingOnChain.join(", ")}`);
        if (extraOnChain.length)
          console.log(`      extra on-chain:   ${extraOnChain.join(", ")}`);
        allMatch = false;
        continue;
      }

      // Compare deployed bytecode (strip metadata for a fair comparison)
      const onChainCode = await provider.getCode(matchingFacet.facetAddress);
      const onChainStripped = stripMetadata(onChainCode);
      const compiledStripped = stripMetadata(artifact.deployedBytecode);

      if (onChainStripped === compiledStripped) {
        console.log(
          `OK    ${artifact.name} @ ${matchingFacet.facetAddress}: selectors and bytecode match`
        );
      } else {
        console.log(
          `FAIL  ${artifact.name} @ ${matchingFacet.facetAddress}: bytecode mismatch (selectors OK)`
        );
        allMatch = false;
      }
    }

    console.log("");
    if (allMatch) {
      console.log("All facets verified successfully.");
    } else {
      console.error(
        "Verification failed: one or more facets do not match compiled artifacts."
      );
      process.exit(1);
    }
  });
