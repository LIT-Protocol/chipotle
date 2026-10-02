// Show the action wallet's confirmed balance and unspent outputs on Zcash
// mainnet (via Blockchair).
//
// There is no testnet faucet path here (Zcash testnet REST infra is dead, so
// this example runs on mainnet) — fund the wallet by sending a little ZEC to
// the address from any wallet or exchange, then run this to confirm it landed.
//
// Usage: npm run balance

const env = require("./_env");
const { runAction } = require("./_lit");
const { getUtxos, getBalance, zatToZec } = require("./_zcash");
env.load();

async function main() {
  const address =
    process.env.ZCASH_ADDRESS || (await runAction({ action: "address" })).address;

  const [balance, utxos] = await Promise.all([
    getBalance(address),
    getUtxos(address),
  ]);

  console.log(`address: ${address}`);
  console.log(`balance: ${zatToZec(balance)} ZEC (${balance} zat)`);
  console.log(`utxos:   ${utxos.length}`);
  for (const u of utxos) {
    console.log(`  - ${u.txid}:${u.vout}  ${zatToZec(u.value)} ZEC`);
  }
  if (utxos.length === 0) {
    console.log("\nNo funds yet. Send a little ZEC to the address above, wait for");
    console.log("a confirmation, then re-run `npm run balance`.");
  }
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
