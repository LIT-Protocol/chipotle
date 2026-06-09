// Attack — exfiltration against the CoW vault, shown three ways.
//
// (1) The compromised bot asks the policy to authorize a settlement, but tries
//     to redirect the payout by changing the order's receiver to itself. The
//     action rebuilds the batch from the *signed* order, so a tampered receiver
//     no longer recovers to the order owner and the action refuses. The only
//     settlement it will sign pays the trader's real receiver.
//
// (2) The bot goes around the action and calls executeSettlement directly with
//     a self-dealing batch and a forged policy signature. The vault recovers the
//     signer, sees it isn't the policy signer, and reverts. Inventory never moves.
//
// (3) The bot tries to call GPv2Settlement.settle directly — but the bot isn't
//     an allowlisted solver (only the vault is), so the settlement rejects it.
//     This is the property that makes the vault-as-solver design necessary.
//
// Usage: node scripts/attack.js   (or: npm run attack)

const { ethers } = require("ethers");
const env = require("./_env");
env.load();
const {
  CHAIN_ID,
  VAULT_ABI,
  SETTLEMENT_ABI,
  policyOrderParam,
  requestSolveAuthorization,
} = require("./_cow");

async function main() {
  for (const k of [
    "ALCHEMY_BASE_SEPOLIA_URL",
    "COW_VAULT_ADDRESS",
    "COW_SETTLEMENT_ADDRESS",
    "COW_ORDER",
    "SOLVER_PRIVATE_KEY",
  ]) {
    if (!process.env[k]) throw new Error(`${k} is required (run setup + order first)`);
  }

  const order = JSON.parse(process.env.COW_ORDER);
  const attacker = process.env.ATTACKER_ADDRESS || "0x000000000000000000000000000000000000dEaD";
  const provider = new ethers.providers.JsonRpcProvider(process.env.ALCHEMY_BASE_SEPOLIA_URL);
  const solver = new ethers.Wallet(process.env.SOLVER_PRIVATE_KEY, provider);

  const authParams = (ord) => ({
    vaultAddress: process.env.COW_VAULT_ADDRESS,
    chainId: CHAIN_ID,
    authDeadline: Math.floor(Date.now() / 1000) + 600,
    order: policyOrderParam(ord, ord.owner, ord.signature),
    rpcUrl: process.env.ALCHEMY_BASE_SEPOLIA_URL,
  });

  // (1) Tamper the receiver, keep the trader's signature.
  console.log("Attack 1: compromised bot rewrites the order receiver to itself...");
  const tampered = { ...order, receiver: attacker };
  const bad = await requestSolveAuthorization(authParams(tampered));
  if (bad && bad.authorized) {
    console.error("  ✗ UNEXPECTED: policy authorized a tampered order. This is a bug.");
    process.exit(1);
  }
  console.log("  ✓ Policy refused:", bad && bad.reason);
  const good = await requestSolveAuthorization(authParams(order));
  console.log(`  The only settlement Lit will sign pays ${good.receiver} (the real receiver),`);
  console.log(`  not the attacker ${ethers.utils.getAddress(attacker)}. Exfiltration impossible.`);

  // (2) Forge a policy signature for a self-dealing batch.
  console.log("\nAttack 2: bot calls executeSettlement with a forged policy signature...");
  const vault = new ethers.Contract(process.env.COW_VAULT_ADDRESS, VAULT_ABI, solver);
  const fakeCalldata = "0xdeadbeef"; // contents don't matter; the sig check fails first
  const pullToken = order.buyToken;
  const pullAmount = order.buyAmount;
  const authDeadline = Math.floor(Date.now() / 1000) + 600;
  const attackerKey = ethers.Wallet.createRandom();
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["bytes32", "address", "uint256", "uint256", "address", "uint256"],
      [
        ethers.utils.keccak256(fakeCalldata),
        pullToken,
        pullAmount,
        authDeadline,
        process.env.COW_VAULT_ADDRESS,
        CHAIN_ID,
      ]
    )
  );
  const forgedSig = await attackerKey.signMessage(ethers.utils.arrayify(digest));
  try {
    await vault.callStatic.executeSettlement(fakeCalldata, pullToken, pullAmount, authDeadline, forgedSig);
    console.error("  ✗ UNEXPECTED: the forged settlement did not revert. This is a bug.");
    process.exit(1);
  } catch (err) {
    console.log("  ✓ Reverted as expected:", err.errorName || err.reason || shortMsg(err));
    console.log("  No valid policy signature -> no settle -> inventory safe.");
  }

  // (3) Bot tries to settle directly on GPv2Settlement — it isn't a solver.
  console.log("\nAttack 3: bot calls GPv2Settlement.settle directly (not allowlisted)...");
  const settlement = new ethers.Contract(process.env.COW_SETTLEMENT_ADDRESS, SETTLEMENT_ABI, solver);
  try {
    await settlement.callStatic.settle([], [], [], [[], [], []]);
    console.error("  ✗ UNEXPECTED: a non-solver settled. This is a bug.");
    process.exit(1);
  } catch (err) {
    console.log("  ✓ Reverted as expected:", err.reason || shortMsg(err));
    console.log("  Only the vault is an allowlisted solver — and it only settles policy-signed batches.");
  }
}

function shortMsg(err) {
  return (err.message || String(err)).split("\n")[0].slice(0, 160);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
