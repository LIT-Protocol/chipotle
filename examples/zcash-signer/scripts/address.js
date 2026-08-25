// Print the action's Zcash t1 address (re-derived live by the action).
//
// Usage: npm run address

const env = require("./_env");
const { runAction } = require("./_lit");
env.load();

async function main() {
  const { address } = await runAction({ action: "address" });
  console.log(address);
}

main().catch((err) => {
  console.error(err.message);
  process.exit(1);
});
