require("@nomicfoundation/hardhat-toolbox");
require("dotenv").config();

const PRIVATE_KEY = process.env.DEPLOYER_PRIVATE_KEY;
// The action reads the vault + settlement contract on this chain, and it
// enforces a hostname whitelist (see action/solverPolicy.js) requiring an
// Alchemy Base-Sepolia URL. Reusing the same URL here keeps the deploy/tx
// path and the action's read path pointed at the same node.
const BASE_SEPOLIA_RPC_URL =
  process.env.ALCHEMY_BASE_SEPOLIA_URL || "https://sepolia.base.org";

module.exports = {
  solidity: {
    version: "0.8.24",
    settings: {
      optimizer: { enabled: true, runs: 200 },
      // Required for OpenZeppelin v5 (uses Cancun's mcopy opcode).
      // Base, Optimism, Arbitrum and Ethereum mainnet all support Cancun.
      evmVersion: "cancun",
    },
  },
  networks: {
    baseSepolia: {
      url: BASE_SEPOLIA_RPC_URL,
      accounts: PRIVATE_KEY ? [PRIVATE_KEY] : [],
    },
  },
};
