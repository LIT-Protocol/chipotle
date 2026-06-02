require("@nomicfoundation/hardhat-toolbox");
require("dotenv").config();

const PRIVATE_KEY = process.env.DEPLOYER_PRIVATE_KEY;
// Same RPC the scripts and the action use, so deploys/reads/broadcasts all
// point at one node.
const RPC_URL = process.env.RPC_URL || "https://sepolia.base.org";

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
      url: RPC_URL,
      accounts: PRIVATE_KEY ? [PRIVATE_KEY] : [],
    },
  },
};
