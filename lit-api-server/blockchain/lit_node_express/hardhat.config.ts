import { HardhatUserConfig } from "hardhat/config";
import "hardhat-diamond-abi"; // Import the plugin

const config: HardhatUserConfig = {
  solidity: {
    version: "0.8.28",
    settings: {
      optimizer: {
        enabled: true,
        runs: 100,
      },
      viaIR: true,
    },
  },
  diamondAbi: {
    // This will create a 'Diamond.json' artifact with ALL facet functions
    name: "AccountConfig",
    include: ["AccountConfigFacets/*Facet.sol"], // Only include contracts ending in 'Facet'
    strict: false,
  },
};

export default config;