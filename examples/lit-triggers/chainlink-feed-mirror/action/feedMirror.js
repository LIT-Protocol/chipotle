// Lit Action: relay a Chainlink price update to a chain Chainlink does not
// natively serve, driven by a lit-triggers CHAIN_EVENT trigger.
//
// The trigger fires on a Chainlink aggregator's AnswerUpdated event on a
// supported source chain (ethereum/base/arbitrum/bsc/polygon). This action
// reads the new (answer, roundId, updatedAt) and writes it to a PriceConsumer
// contract on ANY destination chain reachable by RPC — signed and broadcast by
// the wallet derived from the action's IPFS CID (the consumer pins it as
// `updater`). No trusted relayer federation: the price originates from a
// verifiable on-chain Chainlink event and the relay is signed by a keyless
// wallet tied to this exact code.
//
//   AnswerUpdated(int256 indexed current, uint256 indexed roundId, uint256 updatedAt)
//   -> event.decoded.arg0 = current (price), arg1 = roundId, arg2 = updatedAt
//
// default_params (set on the trigger):
//   destRpcUrl   destination chain RPC
//   destChainId  expected destination chain id (the action refuses to write to
//                any other chain, so a swapped RPC can't redirect the write)
//   consumer     PriceConsumer address on the destination chain
//   gasLimit     optional; explicit so signing never depends on gas estimation
//   dryRun       when true, sign the tx but don't broadcast

const main = async (params) => {
  const event = (params && params.event) || {};
  if (event.source !== "chain_event") {
    return { ok: false, error: "expected chain_event", got: event.source || null };
  }
  const decoded = event.decoded || {};
  const answer = decoded.arg0;
  const roundId = decoded.arg1;
  const updatedAt = decoded.arg2;
  if (answer === undefined || roundId === undefined || updatedAt === undefined) {
    return { ok: false, error: "missing decoded args", decoded };
  }

  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const provider = new ethers.providers.JsonRpcProvider(params.destRpcUrl);

  // Hardening: refuse to write unless the RPC really is the expected dest chain.
  // Stops a swapped/hostile destRpcUrl from redirecting the relayed price.
  const net = await provider.getNetwork();
  if (params.destChainId && net.chainId !== Number(params.destChainId)) {
    return { ok: false, error: "dest_chain_mismatch", expected: Number(params.destChainId), got: net.chainId };
  }

  const signer = wallet.connect(provider);
  const iface = new ethers.utils.Interface([
    "function setPrice(int256 answer, uint256 roundId, uint256 updatedAt)",
  ]);
  const data = iface.encodeFunctionData("setPrice", [answer, roundId, updatedAt]);
  const txReq = { to: params.consumer, data, gasLimit: ethers.BigNumber.from(params.gasLimit || "150000") };

  if (params.dryRun) {
    const populated = await signer.populateTransaction(txReq);
    const signedTx = await signer.signTransaction(populated);
    return {
      ok: true, source_chain: event.chain_key, relayer: wallet.address,
      answer: String(answer), roundId: String(roundId), updatedAt: String(updatedAt),
      dryRun: true, signedTx,
    };
  }
  const tx = await signer.sendTransaction(txReq);
  const receipt = await tx.wait();
  return {
    ok: true, source_chain: event.chain_key, relayer: wallet.address,
    answer: String(answer), roundId: String(roundId), updatedAt: String(updatedAt),
    txHash: receipt.transactionHash, block: receipt.blockNumber,
  };
};
