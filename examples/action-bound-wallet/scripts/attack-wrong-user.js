// Attack: a different user tries to drain someone else's action-bound wallet.
//
//   npm run attack:wrong-user -- <victimIndex> <attackerIndex>
//
// The attacker runs the VICTIM's action (the usage key lets anyone run any
// action in the group) and signs a withdrawal with the ATTACKER's key. The
// action recovers the signer and compares it to the victim's hardcoded
// OWNER_ADDRESS — they don't match, so it refuses to sign. Holding the usage
// key gives you the ability to RUN the action, never to SPEND from its wallet.
//
// Note the deeper point: the attacker can't even reach the victim's funds by
// running their OWN action — their action has a different CID and therefore a
// completely different wallet. There is no path from attacker to victim's
// balance.

const { ethers } = require("ethers");
const env = require("./_env");
const { userWallet } = require("./_users");
const { runUserAction, depositAddressFor } = require("./_lit");
const { withdrawalMessage } = require("./_canonical");

async function main() {
  env.load();
  const victimIndex = process.argv[2] || "0";
  const attackerIndex = process.argv[3] || "1";

  const { RPC_URL, DEMO_TOKEN_ADDRESS } = process.env;
  if (!DEMO_TOKEN_ADDRESS) throw new Error("DEMO_TOKEN_ADDRESS missing (run `npm run setup`)");

  const provider = new ethers.providers.JsonRpcProvider(RPC_URL);
  const victim = userWallet(victimIndex);
  const attacker = userWallet(attackerIndex);

  // The wallet the attacker wants to drain belongs to the VICTIM's action.
  const victimWallet = await depositAddressFor(victim.address);
  console.log(`Victim #${victimIndex} owner:  ${victim.address}`);
  console.log(`Victim wallet (target):  ${victimWallet}`);
  console.log(`Attacker #${attackerIndex} owner: ${attacker.address}`);

  const { chainId } = await provider.getNetwork();
  const nonce = await provider.getTransactionCount(victimWallet, "pending");
  const deadline = Math.floor(Date.now() / 1000) + 600;
  const units = ethers.utils.parseUnits("25", 18).toString();

  // The attacker signs an authorization to pay THEMSELVES.
  const message = withdrawalMessage({
    wallet: victimWallet,
    chainId,
    token: DEMO_TOKEN_ADDRESS,
    to: attacker.address,
    amount: units,
    nonce,
    deadline,
  });
  const signature = await attacker.signMessage(message);

  console.log("\nAttacker runs the victim's action with an attacker-signed authorization...");
  const out = await runUserAction(victim.address, {
    action: "withdraw",
    token: DEMO_TOKEN_ADDRESS,
    to: attacker.address,
    amount: units,
    nonce,
    deadline,
    signature,
    chainId,
    rpcUrl: RPC_URL,
  });

  if (out.ok) {
    console.error("\n✗ UNEXPECTED: the action authorized the attacker. This should never happen.");
    process.exit(1);
  }
  console.log(`\n✓ Rejected as designed: ${out.reason}`);
}

main().catch((err) => {
  console.error(err.message);
  process.exit(1);
});
