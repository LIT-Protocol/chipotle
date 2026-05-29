// schedule-uptime-insurance — parametric "productivity insurance" payout.
//
// Why this is interesting: traditional parametric insurance needs three trusted
// parties — an oracle for the data, a keeper to run the check, and a multisig to
// release funds. This action collapses all three into one trust-minimized unit:
// it fetches the trusted status data, decides, AND signs the payout — from a
// pool key (its own wallet) that no human or server holds. No admin can rug the
// pool and no oracle can be bribed independently of the payout.
//
// Flow (runs on a cron schedule):
//   1. Fetch a Statuspage summary.json (e.g. status.anthropic.com).
//   2. If the overall status indicator is major/critical, the insured service
//      is down — pay out a fixed amount of ETH to the policyholder.
//   3. Otherwise, no-op.
//
// The "pool" is simply this action wallet's balance. Fund it to capitalize the
// policy; the payout drains toward the policyholder when the trigger fires.
//
// Config (default_params):
//   statusUrl     — Statuspage summary.json URL
//   downIndicators — array of indicators that count as "down" (default major/critical)
//   rpcUrl        — payout chain RPC
//   policyholder  — address that gets paid
//   payoutWei     — payout amount in wei
//   dryRun        — when true, sign the payout tx but do not broadcast
//   test_indicator — TEST ONLY: override the fetched indicator to exercise payout

const main = async (params) => {
  const downSet = params.downIndicators || ["major", "critical"];

  let indicator;
  if (params.test_indicator) {
    indicator = params.test_indicator; // test knob to exercise the payout branch
  } else {
    const res = await fetch(params.statusUrl);
    const data = await res.json();
    indicator = (data && data.status && data.status.indicator) || "unknown";
  }

  const down = downSet.indexOf(indicator) !== -1;
  if (!down) {
    return { ok: true, indicator, paid: false, note: "insured service healthy" };
  }

  // Service is down — pay out from the pool (this action's keyless wallet).
  const privateKey = await Lit.Actions.getLitActionPrivateKey();
  const wallet = new ethers.Wallet(privateKey);
  const provider = new ethers.providers.JsonRpcProvider(params.rpcUrl);
  const signer = wallet.connect(provider);
  const txReq = {
    to: params.policyholder,
    value: ethers.BigNumber.from(params.payoutWei),
    gasLimit: ethers.BigNumber.from(params.gasLimit || "21000"),
  };

  if (params.dryRun) {
    const populated = await signer.populateTransaction(txReq);
    const signedTx = await signer.signTransaction(populated);
    return {
      ok: true,
      indicator,
      paid: false,
      dryRun: true,
      pool: wallet.address,
      to: params.policyholder,
      would_pay_wei: String(params.payoutWei),
      signedTx,
    };
  }
  const tx = await signer.sendTransaction(txReq);
  const receipt = await tx.wait();
  return {
    ok: true,
    indicator,
    paid: true,
    pool: wallet.address,
    to: params.policyholder,
    amount_wei: String(params.payoutWei),
    txHash: receipt.transactionHash,
    block: receipt.blockNumber,
  };
};
