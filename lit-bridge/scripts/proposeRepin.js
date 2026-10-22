// Propose a Safe transaction that pins the CURRENT bridge action's CID to the
// production group on the chain-secured account-config diamond (Base mainnet).
//
// The account is chain-secured + Safe-owned, so admin writes (addActionToGroup)
// must come from the Safe. This builds the calldata, wraps it in a SafeTx, signs
// it with the proposer key (a Safe delegate), and POSTs it to the Safe
// Transaction Service. The Safe OWNER then executes it from the Safe UI.
//
// Reads SAFE_PROPOSER_KEY from ../../.context/.env; everything else from ../.env.

const { ethers } = require("ethers");
const fs = require("fs");
const path = require("path");
const env = require("./_env");
const lit = require("./lit");
const { buildAction } = require("./buildAction");

const SAFE_TX_SERVICE = "https://safe-transaction-base.safe.global";
const LIT_API_BASE = "https://api.chipotle.litprotocol.com";

async function main() {
  env.load();
  const SAFE = ethers.utils.getAddress(process.env.SAFE_ADDRESS);
  const GROUP = Number(process.env.GROUP_ID);
  const usageKey = process.env.LIT_USAGE_API_KEY;

  const ctx = fs.readFileSync(path.join(__dirname, "..", "..", ".context", ".env"), "utf8");
  const pk = (ctx.match(/^SAFE_PROPOSER_KEY=(.*)$/m) || [])[1]?.trim();
  if (!pk) throw new Error("SAFE_PROPOSER_KEY missing in .context/.env");
  const proposer = new ethers.Wallet(pk);

  // Diamond address + chain from the node config.
  const cfg = await (await fetch(`${LIT_API_BASE}/core/v1/get_node_chain_config`, {
    headers: { "X-Api-Key": usageKey },
  })).json();
  const DIAMOND = ethers.utils.getAddress(cfg.contract_address);
  const CHAIN = Number(cfg.chain_id);

  // The CID to pin = the current action build (deterministic).
  const code = buildAction(process.env.REGISTRY_ADDRESS, process.env.BRIDGE_PKP_ID);
  const cid = await lit.getActionCid(LIT_API_BASE, usageKey, code);

  const u256 = (h) => ethers.BigNumber.from(h);
  // Use the MASTER apiKeyHash, NOT the keccak256(Safe) alias: addActionToGroup
  // skips the per-group "manage IPFS IDs" permission check only when
  // apiKeyHash == masterHash. The alias hash trips that check (custom error
  // 0xc5a2be52) since the admin has no explicit per-group manage grant. The
  // master hash is already public on-chain (it's the account key + event topic).
  const masterHash = u256(ethers.utils.keccak256(ethers.utils.toUtf8Bytes(process.env.LIT_API_KEY)));
  const cidHash = u256(ethers.utils.keccak256(ethers.utils.toUtf8Bytes(cid)));

  const iface = new ethers.utils.Interface(["function addActionToGroup(uint256,uint256,uint256)"]);
  const data = iface.encodeFunctionData("addActionToGroup", [masterHash, GROUP, cidHash]);

  const A = process.env.ALCHEMY_API_KEY;
  const provider = new ethers.providers.JsonRpcProvider(`https://base-mainnet.g.alchemy.com/v2/${A}`);
  const nonce = (await new ethers.Contract(SAFE, ["function nonce() view returns (uint256)"], provider).nonce()).toNumber();

  const domain = { chainId: CHAIN, verifyingContract: SAFE };
  const types = {
    SafeTx: [
      { name: "to", type: "address" }, { name: "value", type: "uint256" },
      { name: "data", type: "bytes" }, { name: "operation", type: "uint8" },
      { name: "safeTxGas", type: "uint256" }, { name: "baseGas", type: "uint256" },
      { name: "gasPrice", type: "uint256" }, { name: "gasToken", type: "address" },
      { name: "refundReceiver", type: "address" }, { name: "nonce", type: "uint256" },
    ],
  };
  const message = {
    to: DIAMOND, value: 0, data, operation: 0, safeTxGas: 0, baseGas: 0,
    gasPrice: 0, gasToken: ethers.constants.AddressZero,
    refundReceiver: ethers.constants.AddressZero, nonce,
  };
  const safeTxHash = ethers.utils._TypedDataEncoder.hash(domain, types, message);
  const signature = await proposer._signTypedData(domain, types, message);

  console.log("Action CID to pin:", cid);
  console.log("Diamond:", DIAMOND, "| group:", GROUP, "| Safe nonce:", nonce);
  console.log("calldata:", data);
  console.log("safeTxHash:", safeTxHash);

  const body = {
    to: DIAMOND, value: "0", data, operation: 0, safeTxGas: "0", baseGas: "0",
    gasPrice: "0", gasToken: ethers.constants.AddressZero,
    refundReceiver: ethers.constants.AddressZero, nonce,
    contractTransactionHash: safeTxHash,
    sender: ethers.utils.getAddress(proposer.address),
    signature,
    origin: JSON.stringify({ app: "lit-bridge", action: "re-pin hardened action", cid }),
  };
  const url = `${SAFE_TX_SERVICE}/api/v1/safes/${SAFE}/multisig-transactions/`;
  const res = await fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
  console.log("\nSafe Tx Service propose status:", res.status);
  const txt = await res.text();
  if (res.ok || res.status === 201) {
    console.log("✓ Proposed. The Safe owner can now review + execute at:");
    console.log(`  https://app.safe.global/transactions/queue?safe=base:${SAFE}`);
  } else {
    console.log("propose response:", txt.slice(0, 600));
    console.log("\nFallback — owner can execute directly via the Safe UI Transaction Builder:");
    console.log("  to:   ", DIAMOND);
    console.log("  value: 0");
    console.log("  data: ", data);
  }
}

main().catch((e) => { console.error("proposeRepin failed:", e.message); process.exit(1); });
