// chain-feed-mirror — relay a Chainlink price feed to a chain Chainlink does
// not support, with no trusted relayer holding keys.
//
// Why this is interesting: Chainlink publishes feeds on major chains but not on
// every L2 / appchain. This action watches a Chainlink aggregator's
// AnswerUpdated event on a supported source chain (the chain-event trigger),
// reads the new price, and writes it to a PriceConsumer contract on ANY
// destination chain reachable by RPC — signed by a key no human holds. The
// price originates from a verifiable on-chain Chainlink event and the relay is
// trust-minimized.
//
// Note: the chain-event TRIGGER only supports ethereum/base/arbitrum/bsc/polygon
// as the source, but the action BODY can write to any EVM chain via destRpcUrl.
//
// Source event: AnswerUpdated(int256 indexed current, uint256 indexed roundId, uint256 updatedAt)
//   decoded.arg0 = current (price), arg1 = roundId, arg2 = updatedAt
//
// Config (default_params):
//   destRpcUrl — destination chain RPC (the unsupported chain)
//   consumer   — PriceConsumer contract address on the destination chain
//   dryRun     — when true, sign the mirror tx but do not broadcast

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

  const privateKey = await Lit.Actions.getLitActionPrivateKey();
  const wallet = new ethers.Wallet(privateKey);
  const iface = new ethers.utils.Interface([
    "function setPrice(int256 answer, uint256 roundId, uint256 updatedAt)",
  ]);
  const data = iface.encodeFunctionData("setPrice", [answer, roundId, updatedAt]);

  const provider = new ethers.providers.JsonRpcProvider(params.destRpcUrl);
  const signer = wallet.connect(provider);
  const txReq = {
    to: params.consumer,
    data,
    gasLimit: ethers.BigNumber.from(params.gasLimit || "150000"),
  };

  if (params.dryRun) {
    const populated = await signer.populateTransaction(txReq);
    const signedTx = await signer.signTransaction(populated);
    return {
      ok: true,
      source_chain: event.chain_key,
      relayer: wallet.address,
      answer: String(answer),
      roundId: String(roundId),
      updatedAt: String(updatedAt),
      dryRun: true,
      signedTx,
    };
  }
  const tx = await signer.sendTransaction(txReq);
  const receipt = await tx.wait();
  return {
    ok: true,
    source_chain: event.chain_key,
    relayer: wallet.address,
    answer: String(answer),
    roundId: String(roundId),
    updatedAt: String(updatedAt),
    txHash: receipt.transactionHash,
    block: receipt.blockNumber,
  };
};
