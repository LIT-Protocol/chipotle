// Lit Action: relay a Chainlink price update to a chain Chainlink does not
// natively serve, driven by a lit-triggers CHAIN_EVENT trigger.
//
// HARDENING (why this doesn't just trust the trigger): a chain-event trigger
// hands the action a decoded log, but the action is also executable by anyone
// holding the usage key (e.g. via a direct call or a webhook trigger), who
// could supply arbitrary `decoded` values. So this action does NOT trust the
// supplied decode. It re-fetches the transaction receipt from a HOSTNAME-PINNED
// source RPC, confirms the log was emitted by the expected Chainlink aggregator
// with the AnswerUpdated topic, waits for confirmations, and decodes the price
// from the verified log itself. The relay signs only what it independently read
// on the source chain. Editing this file changes the action's IPFS CID and
// therefore its signer, so the PriceConsumer (which pins the relayer) rejects a
// modified action.
//
//   AnswerUpdated(int256 indexed current, uint256 indexed roundId, uint256 updatedAt)
//   -> topics[1] = current (price), topics[2] = roundId, data = updatedAt
//
// Baked-in source policy. These are constants, not params, so a hostile caller
// can't point the action at an attacker-controlled "aggregator" that emits a
// fake AnswerUpdated. To target a different feed/chain, edit these (which
// changes the CID + signer, forcing a PriceConsumer redeploy).
const SOURCE = {
  chainId: 8453, // Base mainnet
  aggregator: "0x1e0b2c3896338fbb201c4f0a27c6904801dca06b", // ETH/USD aggregator (lowercase)
  rpcHost: /^mainnet\.base\.org$/i, // TLS-pinned host; swap for your provider's host
  minConfirmations: 2,
};
const ANSWER_UPDATED_TOPIC = ethers.utils.id("AnswerUpdated(int256,uint256,uint256)");

// default_params (set on the trigger):
//   srcRpcUrl    source-chain RPC; its host must match SOURCE.rpcHost, https only
//   destRpcUrl   destination chain RPC
//   destChainId  REQUIRED expected destination chain id (action refuses to write elsewhere)
//   consumer     PriceConsumer address on the destination chain
//   gasLimit     optional; explicit so signing never depends on gas estimation
//   dryRun       when true, sign the tx but don't broadcast

const main = async (params) => {
  const event = (params && params.event) || {};
  if (event.source !== "chain_event") {
    return { ok: false, error: "expected chain_event", got: event.source || null };
  }
  const txHash = event.transaction_hash;
  const logIndex = event.log_index;
  if (!txHash || logIndex === undefined || logIndex === null) {
    return { ok: false, error: "missing transaction_hash / log_index" };
  }

  // 1. Re-fetch and verify the log from the pinned source RPC.
  const srcUrl = params.srcRpcUrl || "";
  let srcHost;
  try { srcHost = new URL(srcUrl).host; } catch { return { ok: false, error: "bad srcRpcUrl" }; }
  if (!/^https:/i.test(srcUrl) || !SOURCE.rpcHost.test(srcHost)) {
    return { ok: false, error: "source RPC host not allowed", host: srcHost };
  }
  const srcProvider = new ethers.providers.JsonRpcProvider(srcUrl);
  const srcNet = await srcProvider.getNetwork();
  if (srcNet.chainId !== SOURCE.chainId) {
    return { ok: false, error: "source chain mismatch", expected: SOURCE.chainId, got: srcNet.chainId };
  }
  const receipt = await srcProvider.getTransactionReceipt(txHash);
  if (!receipt) return { ok: false, error: "receipt not found", txHash };
  const log = receipt.logs.find((l) => Number(l.logIndex) === Number(logIndex));
  if (!log) return { ok: false, error: "log not found at index", logIndex };
  if (log.address.toLowerCase() !== SOURCE.aggregator) {
    return { ok: false, error: "unexpected log emitter", emitter: log.address };
  }
  if (!log.topics || log.topics[0] !== ANSWER_UPDATED_TOPIC) {
    return { ok: false, error: "not an AnswerUpdated log" };
  }
  const head = await srcProvider.getBlockNumber();
  if (head - receipt.blockNumber < SOURCE.minConfirmations) {
    return { ok: false, error: "insufficient confirmations", need: SOURCE.minConfirmations };
  }

  // 2. Decode from the VERIFIED log (not from event.decoded).
  const answer = ethers.BigNumber.from(log.topics[1]).fromTwos(256); // int256 current
  const roundId = ethers.BigNumber.from(log.topics[2]); // uint256 roundId
  const updatedAt = ethers.BigNumber.from(log.data); // uint256 updatedAt

  // 3. Write to the destination, with a REQUIRED chain-id pin.
  if (!params.destChainId) return { ok: false, error: "destChainId required" };
  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const destProvider = new ethers.providers.JsonRpcProvider(params.destRpcUrl);
  const destNet = await destProvider.getNetwork();
  if (destNet.chainId !== Number(params.destChainId)) {
    return { ok: false, error: "dest_chain_mismatch", expected: Number(params.destChainId), got: destNet.chainId };
  }
  const signer = wallet.connect(destProvider);
  const iface = new ethers.utils.Interface([
    "function setPrice(int256 answer, uint256 roundId, uint256 updatedAt)",
  ]);
  const data = iface.encodeFunctionData("setPrice", [answer, roundId, updatedAt]);
  const txReq = { to: params.consumer, data, gasLimit: ethers.BigNumber.from(params.gasLimit || "150000") };

  const out = {
    ok: true,
    source_chain: SOURCE.chainId,
    aggregator: SOURCE.aggregator,
    relayer: wallet.address,
    answer: answer.toString(),
    roundId: roundId.toString(),
    updatedAt: updatedAt.toString(),
    src_tx: txHash,
  };

  if (params.dryRun) {
    const populated = await signer.populateTransaction(txReq);
    return { ...out, dryRun: true, signedTx: await signer.signTransaction(populated) };
  }
  const tx = await signer.sendTransaction(txReq);
  const rcpt = await tx.wait();
  return { ...out, txHash: rcpt.transactionHash, block: rcpt.blockNumber };
};
