// diamond_deploy.js
// example of how to deploy a new facet to a diamond or upgrade an existing facet

const { ethers } = require('hardhat');

async function main() {
  // 1. Get contract factories
  const Diamond = await ethers.getContractFactory('MyDiamond');
  const ConfigFacet = await ethers.getContractFactory('ConfigFacet');

  // 2. Attach to your already deployed Diamond
  const diamondAddress = '0xYOUR_DIAMOND_ADDRESS_HERE';
  const diamond = await Diamond.attach(diamondAddress);

  // 3. Deploy the new ConfigFacet logic contract
  console.log('Deploying ConfigFacet...');
  const facet = await ConfigFacet.deploy();
  await facet.waitForDeployment();
  const facetAddress = await facet.getAddress();
  console.log('ConfigFacet deployed to:', facetAddress);

  // 4. Prepare the "FacetCut" structure
  // FacetCutAction: 0 = Add, 1 = Replace, 2 = Remove
  const facetCut = [
    {
      facetAddress: facetAddress,
      action: 0, // Add functions
      functionSelectors: [
        facet.interface.getFunction('getGlobalConfig').selector,
        facet.interface.getFunction('setGlobalConfig').selector,
        facet.interface.getFunction('toggleSystem').selector,
      ],
    },
  ];

  // 5. Execute the diamondCut upgrade
  console.log('Adding facet to Diamond...');
  const tx = await diamond.diamondCut(
    facetCut,
    ethers.ZeroAddress, // No initialization contract
    '0x', // No initialization data
  );
  await tx.wait();

  console.log(
    'Diamond upgrade complete! ConfigFacet functions are now active.',
  );
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
