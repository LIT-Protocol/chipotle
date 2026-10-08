// Propose the Ownable2Step acceptOwnership() Safe tx that completes the platform
// governance handoff: the BridgeConfigRegistry -> Safe. transferOwnership was
// already done by handoffToSafe.js (sets pendingOwner = Safe); this proposes the
// Safe's acceptance. Only proposes if a Safe actually exists at the address on the
// registry chain (Safes are per-chain; a missing Safe means the handoff is stuck
// until the Safe is deployed there).
//
// The registry is the ONLY platform-owned contract — BridgeToken contracts are
// per-issuer and handed off (if at all) by their own issuers, so they are NOT
// proposed here. (Keep this in lockstep with handoffToSafe.js, which is also
// registry-only.)
//
// Signs each SafeTx with the proposer/delegate key and POSTs to the chain's Safe
// Transaction Service. The Safe owner then executes.

const { ethers } = require("ethers");
const fs = require("fs");
const path = require("path");
const env = require("./_env");

const SAFE_TX_SERVICE = {
  8453: "https://safe-transaction-base.safe.global",
  42161: "https://safe-transaction-arbitrum.safe.global",
};
const ACCEPT = new ethers.utils.Interface(["function acceptOwnership()"]).encodeFunctionData("acceptOwnership", []);

async function proposeSafeTx(service, safe, chainId, proposer, to, data, nonce, origin) {
  const domain = { chainId, verifyingContract: safe };
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
    to, value: 0, data, operation: 0, safeTxGas: 0, baseGas: 0, gasPrice: 0,
    gasToken: ethers.constants.AddressZero, refundReceiver: ethers.constants.AddressZero, nonce,
  };
  const safeTxHash = ethers.utils._TypedDataEncoder.hash(domain, types, message);
  const signature = await proposer._signTypedData(domain, types, message);
  const body = {
    to, value: "0", data, operation: 0, safeTxGas: "0", baseGas: "0", gasPrice: "0",
    gasToken: ethers.constants.AddressZero, refundReceiver: ethers.constants.AddressZero, nonce,
    contractTransactionHash: safeTxHash, sender: ethers.utils.getAddress(proposer.address),
    signature, origin,
  };
  const res = await fetch(`${service}/api/v1/safes/${safe}/multisig-transactions/`, {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
  });
  return { status: res.status, safeTxHash, text: await res.text() };
}

async function main() {
  env.load();
  const SAFE = ethers.utils.getAddress(process.env.SAFE_ADDRESS);
  const A = process.env.ALCHEMY_API_KEY;
  const ctx = fs.readFileSync(path.join(__dirname, "..", "..", ".context", ".env"), "utf8");
  const pk = (ctx.match(/^SAFE_PROPOSER_KEY=(.*)$/m) || [])[1]?.trim();
  if (!pk) throw new Error("SAFE_PROPOSER_KEY missing");
  const proposer = new ethers.Wallet(pk);

  // The platform-owned contract whose ownership should land at the Safe. Just the
  // registry — tokens are per-issuer (see header). transferOwnership for these was
  // proposed by handoffToSafe.js; proposing acceptOwnership for anything it didn't
  // transfer would just queue a Safe tx that reverts (pendingOwner != Safe).
  if (!process.env.REGISTRY_ADDRESS) throw new Error("REGISTRY_ADDRESS missing");
  const targets = [
    { chainId: 8453, sub: "base-mainnet", label: "registry (Base)", addr: process.env.REGISTRY_ADDRESS },
  ];

  // Group by chain so we can assign sequential Safe nonces per chain.
  const byChain = {};
  for (const t of targets) (byChain[t.chainId] ||= []).push(t);

  for (const chainId of Object.keys(byChain).map(Number)) {
    const sub = byChain[chainId][0].sub;
    const provider = new ethers.providers.JsonRpcProvider(`https://${sub}.g.alchemy.com/v2/${A}`);
    const code = await provider.getCode(SAFE);
    if (code.length <= 2) {
      console.log(`\n⚠ chain ${chainId}: NO Safe deployed at ${SAFE} — cannot accept ownership here. Skipping:`);
      for (const t of byChain[chainId]) console.log(`   - ${t.label} (${t.addr}) stays pending until a Safe exists at ${SAFE} on this chain`);
      continue;
    }
    const service = SAFE_TX_SERVICE[chainId];
    let nonce = (await new ethers.Contract(SAFE, ["function nonce() view returns (uint256)"], provider).nonce()).toNumber();
    for (const t of byChain[chainId]) {
      const r = await proposeSafeTx(service, SAFE, chainId, proposer, ethers.utils.getAddress(t.addr), ACCEPT, nonce,
        JSON.stringify({ app: "lit-bridge", action: "acceptOwnership", target: t.label }));
      if (r.status === 201) {
        console.log(`✓ proposed acceptOwnership for ${t.label} @ nonce ${nonce} — safeTxHash ${r.safeTxHash}`);
        nonce++;
      } else {
        console.log(`✗ ${t.label} @ nonce ${nonce}: ${r.status} ${r.text.slice(0, 200)}`);
      }
    }
    console.log(`Execute queued txs: https://app.safe.global/transactions/queue?safe=${chainId === 8453 ? "base" : "arb1"}:${SAFE}`);
  }
}

main().catch((e) => { console.error("proposeAccepts failed:", e.message); process.exit(1); });
