// Attack 1 — exfiltration. The bot is compromised: the attacker has the Lit
// usage key and full control of the bot's environment. They try to authorize a
// fill that pays THEM instead of the order's real recipient.
//
// They can put any recipient they like in js_params. But the policy action
// reads the order on-chain and binds the fill to the order's actual recipient
// — which the attacker can't rewrite. So the action refuses to sign, and no
// signature means executeFill can't move a cent. Inventory is safe.
//
// Usage: node scripts/attack-exfiltrate.js   (or: npm run attack:exfiltrate)

const env = require("./_env");
env.load();
const { requestFillAuthorization, fillParams } = require("./_lit");

async function main() {
  const attacker = process.env.ATTACKER_ADDRESS || "0x000000000000000000000000000000000000dEaD";
  console.log(`Compromised bot tries to redirect the 100 mUSDC fill -> ${attacker}`);

  const auth = await requestFillAuthorization(fillParams({ recipient: attacker }));

  if (auth && auth.authorized) {
    console.error("\n✗ UNEXPECTED: policy authorized the exfiltration. This is a bug.");
    console.error(auth);
    process.exit(1);
  }

  console.log("\n✓ Policy REJECTED the exfiltration. No signature was produced.");
  console.log("  reason:", auth && auth.reason);
  console.log("  executeFill has nothing to submit — inventory never moved.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
