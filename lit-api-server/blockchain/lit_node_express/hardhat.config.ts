import { HardhatUserConfig } from "hardhat/config";
import "hardhat-diamond-abi"; // Import the plugin
import "@nomicfoundation/hardhat-ethers";
import "dotenv/config";

import "./tasks/propose-diamond-cut";
import "./tasks/execute-safe-tx";
import "./tasks/propose-dstack-app";
import "./tasks/transfer-ownership";

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
  networks: {
    base: {
      url: process.env.BASE_RPC_URL || "https://mainnet.base.org",
      chainId: 8453,
      // No accounts - we use Safe for privileged operations
    },
    "base-sepolia": {
      url: process.env.BASE_SEPOLIA_RPC_URL || "https://sepolia.base.org",
      chainId: 84532,
    },
  },
};

export default config;
