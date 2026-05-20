// Lit Action: observe a BurnInitiated event on a source chain via
// eth_getTransactionReceipt, validate the burn against a known BridgeToken
// deployment, and sign a mint authorization for the destination chain.
//
// This is the permissionless half of a burn/mint cross-chain token. Anyone
// who saw the burn happen can call this action with the burn tx hash; the
// signature is what authorizes the mint, not the caller's identity. The
// signer key is derived from this action's IPFS CID
// (Lit.Actions.getLitActionPrivateKey), so the deployed BridgeToken
// contracts trust this exact source — edit the action by a byte and the
// signer changes and every deployed contract refuses the modified action.
//
// js_params:
//   burnTxHash         tx hash of the BridgeToken.burn() call on the source
//   srcChainId         chain id where the burn happened
//   srcRpcUrl          RPC URL for the source chain — must match the
//                      hostname whitelist for that chain (see RPC_HOSTS)
//   srcContract        BridgeToken address on the source chain (the contract
//                      that emitted BurnInitiated). The mint contract on the
//                      destination side has an independent `bridgePartner`
//                      mapping that pins the trusted source per chain id;
//                      lying here lands you a signature that the destination
//                      contract rejects.
//   destChainId        chain id where the mint contract lives
//   destContract       BridgeToken address on the destination chain
//   logIndex           index of the BurnInitiated log within the burn tx's
//                      receipt — needed because a single tx could emit
//                      multiple BurnInitiated events
//   deadline           unix seconds; the destination contract rejects after

// Per-chain RPC policy. `host` is the accepted hostname (TLS guarantees the
// body came from there). `minConfirmations` is how many blocks the burn tx
// must be buried under before we'll sign, to defang reorgs that would let
// a user keep their source-chain balance AND mint on the destination.
//
// Edit this table to add chains; doing so changes the action's IPFS CID
// and therefore the signer address, forcing redeploys of every existing
// BridgeToken. 84532 = Base Sepolia, 421614 = Arbitrum Sepolia. Both are
// rollups with fast soft finality; 5 blocks is overkill for normal
// conditions and still very fast in wall-clock time (~10s on Base Sepolia,
// ~1s on Arb Sepolia). Production deployments on chains with deeper
// reorg risk (L1 mainnet, BNB, etc.) want substantially higher.
//
// Public RPCs often share hostnames between chains (`rpc.ankr.com` etc.),
// which is precisely what the hostname pin is here to rule out.
const RPC_HOSTS = {
  84532: {
    host: /^base-sepolia\.g\.alchemy\.com$/i,
    minConfirmations: 5,
  },
  421614: {
    host: /^arb-sepolia\.g\.alchemy\.com$/i,
    minConfirmations: 5,
  },
};

async function main({
  burnTxHash,
  srcChainId,
  srcRpcUrl,
  srcContract,
  destChainId,
  destContract,
  logIndex,
  deadline,
}) {
  // ---- 1. Hostname + scheme whitelist for the source RPC ------------------
  // HTTPS is mandatory: TLS is the trust anchor for "body came from the
  // named host." Plain http:// would let a network attacker on the path
  // return a fake receipt while still passing the hostname pin.
  const policy = RPC_HOSTS[Number(srcChainId)];
  if (!policy) {
    return {
      authorized: false,
      reason: `srcChainId ${srcChainId} not in RPC_HOSTS whitelist`,
    };
  }
  let parsed;
  try {
    parsed = new URL(srcRpcUrl);
  } catch {
    return { authorized: false, reason: "srcRpcUrl is not a valid URL" };
  }
  if (parsed.protocol !== "https:") {
    return {
      authorized: false,
      reason: `srcRpcUrl must use https:// (got ${parsed.protocol})`,
    };
  }
  if (!policy.host.test(parsed.hostname)) {
    return {
      authorized: false,
      reason: `srcRpcUrl host ${parsed.hostname} does not match whitelist for chain ${srcChainId}`,
    };
  }

  // ---- 2. Independent chainId check ---------------------------------------
  // Belt-and-suspenders with the hostname check above: an Alchemy URL with
  // the right hostname could still be pointed at a wrong project, etc. Cheap
  // sanity check.
  const reportedChainId = await rpc(srcRpcUrl, "eth_chainId", []);
  if (BigInt(reportedChainId) !== BigInt(srcChainId)) {
    return {
      authorized: false,
      reason: `RPC reports chainId ${BigInt(reportedChainId)} but srcChainId says ${srcChainId}`,
    };
  }

  // ---- 3. Pull the receipt and find the matching BurnInitiated log --------
  const receipt = await rpc(srcRpcUrl, "eth_getTransactionReceipt", [
    burnTxHash,
  ]);
  if (!receipt) {
    return { authorized: false, reason: "burn tx not found or not yet mined" };
  }
  if (BigInt(receipt.status) !== 1n) {
    return { authorized: false, reason: "burn tx reverted" };
  }

  // ---- 3a. Finality check -------------------------------------------------
  // If we sign before the burn is finalized, a reorg can pull the burn out
  // of history while the mint signature is already issued — user keeps
  // source-chain tokens AND mints on the destination. Wait for the burn
  // block to be buried under `minConfirmations` blocks. Per-chain count
  // lives in RPC_HOSTS.
  const headBlock = BigInt(await rpc(srcRpcUrl, "eth_blockNumber", []));
  const burnBlock = BigInt(receipt.blockNumber);
  const confirmations = headBlock - burnBlock;
  if (confirmations < BigInt(policy.minConfirmations)) {
    return {
      authorized: false,
      reason: `burn has ${confirmations} confirmations, need ${policy.minConfirmations} on chain ${srcChainId}`,
    };
  }

  const idx = Number(logIndex);
  // logIndex is the index within the FULL block — match by global index, not
  // by receipt.logs[idx]. The caller could supply either; we accept either
  // shape and search by `logIndex` field.
  const log = receipt.logs.find((l) => Number(l.logIndex) === idx);
  if (!log) {
    return {
      authorized: false,
      reason: `no log with logIndex ${idx} on burn tx`,
    };
  }

  // ---- 4. Validate the log came from the named source contract ------------
  if (log.address.toLowerCase() !== srcContract.toLowerCase()) {
    return {
      authorized: false,
      reason: `log was emitted by ${log.address}, not srcContract ${srcContract}`,
    };
  }
  // Topic[0] must be the BurnInitiated signature. We compute it from the
  // canonical signature string instead of hardcoding to keep this readable;
  // if anyone changes the event signature in BridgeToken.sol, the action
  // stops working until the constant is updated (and that update mints a
  // new CID, which is the intended trust property).
  const expectedTopic = ethers.utils.id(
    "BurnInitiated(address,address,uint256,uint256,uint256)"
  );
  if ((log.topics[0] || "").toLowerCase() !== expectedTopic.toLowerCase()) {
    return {
      authorized: false,
      reason: "log is not a BurnInitiated event",
    };
  }

  // ---- 5. Decode topics + data -------------------------------------------
  // BurnInitiated(address indexed from, address indexed recipient,
  //               uint256 amount, uint256 indexed destChainId, uint256 nonce)
  // → topics: [sig, from, recipient, destChainId], data: amount + nonce
  const recipient = ethers.utils.getAddress("0x" + log.topics[2].slice(26));
  const logDestChainId = BigInt(log.topics[3]);
  const decoded = ethers.utils.defaultAbiCoder.decode(
    ["uint256", "uint256"],
    log.data
  );
  const amount = decoded[0]; // BigNumber
  const srcNonce = decoded[1]; // BigNumber

  // ---- 6. The recipient + destination encoded in the burn event must match
  // what the caller is asking us to sign. (The recipient is in the burn
  // event, so the caller can't redirect it; this just surfaces a clearer
  // error than "signature didn't recover.")
  if (logDestChainId !== BigInt(destChainId)) {
    return {
      authorized: false,
      reason: `burn targets chainId ${logDestChainId}, not destChainId ${destChainId}`,
    };
  }

  // ---- 7. Sign ------------------------------------------------------------
  // The destination BridgeToken contract independently verifies (a) the
  // source contract matches its `bridgePartner[srcChainId]` and (b) the
  // signature is from `bridgeOracle`. We pack all the parameters here so
  // the on-chain recovery has the same preimage.
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      [
        "uint256", // srcChainId
        "address", // srcContract
        "bytes32", // burnTxHash
        "uint256", // logIndex
        "address", // recipient
        "uint256", // amount
        "uint256", // srcNonce
        "uint256", // deadline
        "address", // destContract  (mint contract == address(this))
        "uint256", // destChainId   (block.chainid on dest)
      ],
      [
        srcChainId,
        srcContract,
        burnTxHash,
        logIndex,
        recipient,
        amount,
        srcNonce,
        deadline,
        destContract,
        destChainId,
      ]
    )
  );

  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const signature = await wallet.signMessage(ethers.utils.arrayify(digest));

  return {
    authorized: true,
    signature,
    signer: wallet.address,
    srcChainId,
    srcContract,
    burnTxHash,
    logIndex,
    recipient,
    amount: amount.toString(),
    srcNonce: srcNonce.toString(),
    destChainId,
    destContract,
    deadline,
  };
}

async function rpc(url, method, params) {
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const body = await res.json();
  if (body.error) throw new Error(`${method} -> ${body.error.message}`);
  return body.result;
}
