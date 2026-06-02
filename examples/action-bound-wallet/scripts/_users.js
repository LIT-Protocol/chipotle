// Demo users + the per-user action source builder.
//
// ┌──────────────────────────────────────────────────────────────────────┐
// │  THESE PRIVATE KEYS ARE PUBLIC TEST KEYS. NEVER PUT REAL FUNDS ON THEM. │
// └──────────────────────────────────────────────────────────────────────┘
//
// They are the well-known Hardhat / Anvil default accounts — printed in every
// local-node startup log on earth. We use them so the demo runs with zero
// setup and it is unmistakable that they hold nothing of value. In a real app
// each "user" is just some EOA address you know (from a login, a connected
// wallet, a database) — you never need their private key, only their address.

const { ethers } = require("ethers");
const fs = require("fs");
const path = require("path");

const ACTION_TEMPLATE = path.join(__dirname, "..", "action", "userWallet.js");

// Public Hardhat/Anvil test accounts. index → private key.
const USERS = [
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80", // #0
  "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d", // #1
  "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a", // #2
];

// Resolve a user index (default 0) into its signing wallet (the owner EOA).
function userWallet(index = 0) {
  const pk = USERS[Number(index)];
  if (!pk) {
    throw new Error(`no demo user at index ${index} (have 0..${USERS.length - 1})`);
  }
  return new ethers.Wallet(pk);
}

// Build the per-user action source by stamping the owner's address into the
// template. This is the heart of the pattern: a different address produces a
// byte-different file, which gets a different IPFS CID, which derives a
// different wallet. One template, one immutable wallet per user.
function actionSourceFor(ownerAddress) {
  const checksummed = ethers.utils.getAddress(ownerAddress);
  const template = fs.readFileSync(ACTION_TEMPLATE, "utf8");
  if (!template.includes("__OWNER_ADDRESS__")) {
    throw new Error("action template is missing the __OWNER_ADDRESS__ placeholder");
  }
  // Replace every occurrence — the placeholder appears in the doc comment as
  // well as the real constant, and they must agree.
  return template.split("__OWNER_ADDRESS__").join(checksummed);
}

module.exports = { USERS, userWallet, actionSourceFor };
