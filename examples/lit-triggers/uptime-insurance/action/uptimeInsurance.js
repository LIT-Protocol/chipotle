// Lit Action: parametric "uptime insurance" payout, driven by a lit-triggers
// SCHEDULE trigger.
//
// On each cron tick this fetches a Statuspage summary; if the monitored service
// is in a major/critical incident, it pays a fixed amount of ETH to the
// policyholder — signed and broadcast by the action's own wallet, a key no
// human holds. That wallet's balance IS the insurance pool. Fund it to
// capitalize the policy.
//
// Why it's interesting: parametric insurance normally needs three trusted
// parties — an oracle (the data), a keeper (runs the check), and a multisig
// (releases funds). This collapses all three into one trust-minimized unit. No
// admin can rug the pool and no oracle can be bribed independently of the
// payout: the same content-addressed code both reads the status and signs the
// payout.
//
// default_params (set on the trigger):
//   statusUrl      Statuspage summary.json URL
//   downIndicators array of indicators that count as "down" (default major/critical)
//   rpcUrl         payout chain RPC
//   policyholder   address that gets paid
//   payoutWei      payout amount in wei
//   gasLimit       optional; explicit so signing never depends on gas estimation
//   dryRun         when true, sign the payout but don't broadcast
//   test_indicator TEST ONLY: override the fetched indicator to exercise payout

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

  if (downSet.indexOf(indicator) === -1) {
    return { ok: true, indicator, paid: false, note: "insured service healthy" };
  }

  // Service is down — pay out from the pool (this action's keyless wallet).
  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
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
