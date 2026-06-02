// Attack — exfiltration against the Across vault, shown two ways.
//
// (1) The compromised bot asks the policy action to authorize a fill, hoping to
//     redirect funds to itself. But acrossPolicy reconstructs the relay entirely
//     from the on-chain deposit — there is no caller-supplied recipient to
//     tamper with. The only relay it will ever sign pays the deposit's real
//     recipient. We print both addresses to show the attacker gains nothing.
//
// (2) The bot goes around the action and calls executeAcrossFill directly with
//     a self-dealing relay and a forged signature. The vault recovers the signer
//     and it isn't the policy signer, so it reverts. Inventory never moves.
//
// Usage: node scripts/across-attack.js   (or: npm run across:attack)

const { ethers } = require("ethers");
const env = require("./_env");
env.load();
const {
  VAULT_FILL_ABI,
  DEST_WETH,
  RELAY_DATA_TUPLE,
  requestAcrossAuthorization,
  authParams,
  relayTuple,
} = require("./_across");

async function main() {
  const attacker = process.env.ATTACKER_ADDRESS || "0x000000000000000000000000000000000000dEaD";

  // (1) Ask the action to authorize; inspect who the signed relay actually pays.
  console.log("Attack 1: compromised bot requests authorization, hoping to be paid...");
  const auth = await requestAcrossAuthorization(authParams());
  if (!auth || !auth.authorized) {
    console.log("  policy declined outright:", auth && auth.reason);
  } else {
    const paid = ethers.utils.getAddress(auth.relayData.recipient);
    const atk = ethers.utils.getAddress(attacker);
    console.log(`  signed relay pays:  ${paid}`);
    console.log(`  attacker address:   ${atk}`);
    if (paid === atk) {
      console.error("  ✗ UNEXPECTED: the relay pays the attacker. This is a bug.");
      process.exit(1);
    }
    console.log("  ✓ The only relay Lit will sign pays the real recipient. Exfiltration impossible.");
  }

  // (2) Go around the action: forge a self-dealing relay and sign it with the
  //     attacker's own key (a well-formed signature, just from the wrong
  //     signer). The vault recovers the signer, sees it isn't the policy
  //     signer, and reverts with InvalidPolicySignature.
  console.log("\nAttack 2: bot calls executeAcrossFill directly with an attacker-signed relay...");
  const provider = new ethers.providers.JsonRpcProvider(process.env.ALCHEMY_BASE_SEPOLIA_URL);
  const solver = new ethers.Wallet(process.env.SOLVER_PRIVATE_KEY, provider);
  const vault = new ethers.Contract(process.env.ACROSS_VAULT_ADDRESS, VAULT_FILL_ABI, solver);
  const chainId = (await provider.getNetwork()).chainId;

  const selfDealing = {
    depositor: attacker,
    recipient: attacker,
    exclusiveRelayer: ethers.constants.AddressZero,
    inputToken: DEST_WETH,
    outputToken: DEST_WETH,
    inputAmount: ethers.utils.parseEther("0.0001").toString(),
    outputAmount: ethers.utils.parseEther("0.0001").toString(),
    originChainId: "11155111",
    depositId: 1,
    fillDeadline: Math.floor(Date.now() / 1000) + 600,
    exclusivityDeadline: 0,
    message: "0x",
  };
  const repaymentChainId = 84532;
  const authDeadline = Math.floor(Date.now() / 1000) + 600;

  // Attacker signs the exact digest the vault checks — but with their own key.
  const attackerKey = ethers.Wallet.createRandom();
  const encoded = ethers.utils.defaultAbiCoder.encode(
    [RELAY_DATA_TUPLE, "uint256", "uint256", "address", "uint256"],
    [relayTuple(selfDealing), repaymentChainId, authDeadline, process.env.ACROSS_VAULT_ADDRESS, chainId]
  );
  const forgedSig = await attackerKey.signMessage(
    ethers.utils.arrayify(ethers.utils.keccak256(encoded))
  );

  try {
    await vault.callStatic.executeAcrossFill(
      relayTuple(selfDealing),
      repaymentChainId,
      authDeadline,
      forgedSig
    );
    console.error("  ✗ UNEXPECTED: the forged fill did not revert. This is a bug.");
    process.exit(1);
  } catch (err) {
    const name = err.errorName || (err.reason || err.message);
    console.log("  ✓ Reverted as expected:", name);
    console.log("  No valid policy signature -> no fill -> inventory safe.");
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
