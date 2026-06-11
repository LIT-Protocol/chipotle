require("@nomicfoundation/hardhat-toolbox");
require("dotenv").config();

const PRIVATE_KEY = process.env.DEPLOYER_PRIVATE_KEY;
// Everything in this example lives on Base Sepolia: our own GPv2Settlement, the
// solver vault, and the two test tokens. (We deploy our own settlement, so any
// EVM chain works; Base Sepolia is fast + cheap.) The policy action reads the
// vault + settlement on this chain and enforces a hostname whitelist (see
// action/cowPolicy.js) requiring an Alchemy Base-Sepolia URL. Reusing the same
// URL here keeps the deploy/tx path and the action's read path on the same node.
const BASE_SEPOLIA_RPC_URL =
  process.env.ALCHEMY_BASE_SEPOLIA_URL || "https://sepolia.base.org";

module.exports = {
  solidity: {
    version: "0.8.24",
    settings: {
      optimizer: { enabled: true, runs: 200 },
      // Required for OpenZeppelin v5 (uses Cancun's mcopy opcode).
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
