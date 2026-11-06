// Requires ethers v6. Temporary fallback for the AccountConfig diamond layout.
// Use the DIAMOND/PROXY address, not a facet address. No signer or API secret needed.
// Replace with isAccountManaged(uint256) after that getter is deployed.
const { AbiCoder, id, keccak256 } = require("ethers");

// Chipotle production on Base mainnet (chain ID 8453).
// Source: lit-api-server/NodeConfig.prod.toml.
const CHIPOTLE_PROD_DIAMOND = "0xaaaaa9120fe271f653cfdb6bf400db93d2dea7aa";
const BASE_RPC_URL = "https://mainnet.base.org";

const coder = AbiCoder.defaultAbiCoder();
const ROOT = BigInt(id("com.litprotocol.accountconfig.storage"));
const slotFor = (key, slot) =>
  BigInt(keccak256(coder.encode(["uint256", "uint256"], [key, slot])));

async function readAccountManaged(provider, diamondAddress, apiKeyHash, blockTag) {
  // Pin every read to the same block height, including key-to-account resolution.
  blockTag ??= await provider.send("eth_blockNumber", []);
  if ((await provider.getCode(diamondAddress, blockTag)) === "0x") {
    throw new Error("No contract at the supplied address/block");
  }
  const read = async (slot) =>
    BigInt(await provider.getStorage(diamondAddress, slot, blockTag));

  // AccountConfigStorage.allApiKeyHashesToMaster: root + 2.
  const master = await read(slotFor(BigInt(apiKeyHash), ROOT + 2n));
  if (master === 0n) throw new Error("Unknown or removed account/key hash");

  // AccountConfigStorage.accounts: root + 0.
  const account = slotFor(master, ROOT);
  // Account.accountApiKey.apiKeyHash: account + 3.
  if ((await read(account + 3n)) !== master) {
    throw new Error("Account storage mismatch; verify address and storage layout");
  }
  // Account.managed: account + 27, byte offset 0 (least-significant byte).
  const managed = (await read(account + 27n)) & 0xffn;
  if (managed > 1n) throw new Error("Unexpected boolean; verify storage layout");
  return managed === 1n;
}

module.exports = { readAccountManaged, CHIPOTLE_PROD_DIAMOND, BASE_RPC_URL };

// Example:
// const { JsonRpcProvider } = require("ethers");
// const provider = new JsonRpcProvider(BASE_RPC_URL);
// const managed = await readAccountManaged(provider, CHIPOTLE_PROD_DIAMOND, apiKeyHash);
// console.log(managed ? "API mode" : "ChainSecured");
// Offsets depend on the deployed storage layout; this is not an upgrade-stable API.
