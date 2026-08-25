// Send ZEC from the action's keyless transparent wallet:
//   1. Fetch the wallet's UTXOs and select enough to cover amount + fee.
//   2. Ask the action to build + sign the transaction. The action validates
//      the spend (recipient is a real t1 address, amount under cap, change can
//      only return to itself), constructs every output, computes the ZIP-243
//      sighash for each input, secp256k1-signs them, and returns the raw hex.
//   3. Broadcast that hex to Zcash mainnet via Blockchair.
//
// The action never sees a private key leave the TEE and never trusts us to set
// the outputs — it builds them itself. We only choose which UTXOs to spend and
// what fee to pay; lying about either can only make the broadcast fail, never
// redirect funds (input values and outpoints are committed in the sighash).
//
// Usage:
//   npm run transfer -- <t1recipient> <amountZec> [feeZec]
//   npm run transfer -- t1abc...recipient 0.001
//
// Try `npm run transfer -- <recipient> 0.05` to watch the action REFUSE
// anything over the 0.01 ZEC cap baked into its (CID-bound) source.

const env = require("./_env");
const { runAction } = require("./_lit");
const { getUtxos, getTipHeight, broadcast, zecToZat, zatToZec } = require("./_zcash");
env.load();

// Default miner fee: 0.0001 ZEC. Comfortably above the ZIP-317 minimum for a
// small transparent transaction; the action caps the fee at 0.0005 ZEC.
const DEFAULT_FEE_ZAT = 10_000;

async function main() {
  const recipient = process.argv[2];
  const amountZec = Number(process.argv[3]);
  const feeZat = process.argv[4] ? zecToZat(process.argv[4]) : DEFAULT_FEE_ZAT;
  if (!recipient || !Number.isFinite(amountZec) || amountZec <= 0) {
    throw new Error("Usage: npm run transfer -- <t1recipient> <amountZec> [feeZec]");
  }
  const amountZat = zecToZat(amountZec);

  const from =
    process.env.ZCASH_ADDRESS || (await runAction({ action: "address" })).address;

  // ---- 1. Select inputs ---------------------------------------------------
  const [utxos, tipHeight] = await Promise.all([getUtxos(from), getTipHeight()]);
  if (utxos.length === 0) {
    throw new Error(`wallet ${from} has no UTXOs — fund it first (npm run balance)`);
  }

  // Greedy largest-first selection until we cover amount + fee.
  const target = amountZat + feeZat;
  const sorted = [...utxos].sort((a, b) => Number(b.value) - Number(a.value));
  const inputs = [];
  let total = 0;
  for (const u of sorted) {
    inputs.push(u);
    total += Number(u.value);
    if (total >= target) break;
  }
  if (total < target) {
    throw new Error(
      `insufficient funds: have ${zatToZec(total)} ZEC, need ${zatToZec(target)} ZEC (amount + fee)`
    );
  }

  // nExpiryHeight: a window past the current tip so the tx has time to confirm.
  const expiryHeight = tipHeight + 40;

  // ---- 2. Have the action build + sign it ---------------------------------
  console.log(`Asking the action to sign: ${amountZec} ZEC  ${from} -> ${recipient}`);
  console.log(`  inputs: ${inputs.length}, fee: ${zatToZec(feeZat)} ZEC, expiry height: ${expiryHeight}`);
  const result = await runAction({
    action: "sign",
    inputs,
    recipient,
    amountZat: String(amountZat),
    feeZat: String(feeZat),
    expiryHeight,
  });
  if (!result || !result.authorized) {
    console.error("Action declined to sign:", result && result.reason);
    process.exit(2);
  }
  console.log(
    `  signed: amount ${zatToZec(result.amountZat)} ZEC, change ${zatToZec(result.changeZat)} ZEC, fee ${zatToZec(result.feeZat)} ZEC`
  );

  // ---- 3. Broadcast -------------------------------------------------------
  const txid = await broadcast(result.txHex);
  console.log(`broadcast tx: ${txid}`);
  console.log(`explorer:     https://blockchair.com/zcash/transaction/${txid}`);
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
