import fs from "fs";
import path from "path";
import { ethers } from "hardhat";
import type { Signer } from "ethers";

enum FacetCutAction {
  Add = 0,
  Replace = 1,
  Remove = 2,
}

type FacetCut = {
  facetAddress: string;
  action: FacetCutAction;
  functionSelectors: string[];
};

const PREBUILT_DIR = path.resolve(
  __dirname,
  "../../../rust_generator_and_deployer/src/diamond",
);

function loadPrebuilt(name: string): { abi: any[]; bytecode: string } {
  const p = path.join(PREBUILT_DIR, `${name}.json`);
  const raw = JSON.parse(fs.readFileSync(p, "utf8"));
  const bytecode = typeof raw.bytecode === "string"
    ? raw.bytecode
    : raw.bytecode?.object;
  if (!bytecode) throw new Error(`No bytecode in prebuilt artifact ${p}`);
  return { abi: raw.abi, bytecode };
}

async function deployByName(
  name: string,
  signer: Signer,
): Promise<{ address: string; selectors: string[]; iface: any }> {
  const factory = await ethers.getContractFactory(name, signer);
  const facet = await factory.deploy();
  await facet.waitForDeployment();
  const selectors = facet.interface.fragments
    .filter((f: any) => f.type === "function")
    .map((f: any) => facet.interface.getFunction(f.name)!.selector);
  return {
    address: await facet.getAddress(),
    selectors,
    iface: facet.interface,
  };
}

async function deployPrebuilt(
  name: string,
  signer: Signer,
): Promise<{ address: string; selectors: string[]; iface: any }> {
  const { abi, bytecode } = loadPrebuilt(name);
  const factory = new ethers.ContractFactory(abi, bytecode, signer);
  const c = await factory.deploy();
  await c.waitForDeployment();
  const selectors = c.interface.fragments
    .filter((f: any) => f.type === "function")
    .map((f: any) => c.interface.getFunction(f.name)!.selector);
  return { address: await c.getAddress(), selectors, iface: c.interface };
}

/**
 * Deploy the AccountConfig diamond with all facets cut in. Returns the diamond
 * address — call individual facet interfaces against this address via
 * `ethers.getContractAt("WritesFacet", address)`.
 */
export async function deployAccountConfig(owner: Signer): Promise<string> {
  const ownerAddress = await owner.getAddress();
  const cuts: FacetCut[] = [];

  // Diamond infrastructure facets ship as pre-built JSON in the Rust deployer
  // tree (Hardhat's `paths.sources` doesn't include `libraries/diamond/`).
  for (const name of ["DiamondCutFacet", "DiamondLoupeFacet", "OwnershipFacet"]) {
    const { address, selectors } = await deployPrebuilt(name, owner);
    cuts.push({
      facetAddress: address,
      action: FacetCutAction.Add,
      functionSelectors: selectors,
    });
  }

  // Application facets compiled by Hardhat from contracts/AccountConfigFacets.
  for (const name of [
    "APIConfigFacet",
    "BillingFacet",
    "ViewsFacet",
    "WritesFacet",
  ]) {
    const { address, selectors } = await deployByName(name, owner);
    cuts.push({
      facetAddress: address,
      action: FacetCutAction.Add,
      functionSelectors: selectors,
    });
  }

  const InitFactory = await ethers.getContractFactory("DiamondInit", owner);
  const init = await InitFactory.deploy();
  await init.waitForDeployment();
  const initSelector = init.interface.getFunction("init")!.selector;

  const AccountConfig = await ethers.getContractFactory(
    "contracts/AccountConfig.sol:AccountConfig",
    owner,
  );
  const diamond = await AccountConfig.deploy(
    ownerAddress,
    cuts,
    await init.getAddress(),
    initSelector,
  );
  await diamond.waitForDeployment();
  return diamond.getAddress();
}
