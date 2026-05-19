// Lit Action: read an EVM view function from three independent RPC providers
// in parallel and only sign the result when all three return byte-identical
// data at the same block.
//
// Two cryptographic identities are at play in this action and they do
// different things:
//
//   * The action's OWN derived key (Lit.Actions.getLitActionPrivateKey) is
//     what produces the signature the on-chain ConsensusOracle registry
//     trusts. The key is deterministically derived from this action's IPFS
//     CID — so the registry trusts this exact code, not "anyone who happens
//     to hold the right PKP." Editing this file by a single byte produces a
//     new CID, a new derived key, and a new address; the deployed registry
//     stops accepting signatures from the modified action.
//
//   * The decrypt PKP (`decryptPkpId`) is solely the encryption boundary
//     for the three RPC URLs. Encrypt/Decrypt in Lit are PKP-keyed, so we
//     need a PKP just for that. It signs nothing the registry cares about.
//
// Whitelisting note: anyone who has access to a usage key for this PKP can
// call Lit.Actions.Encrypt for it, so encryption alone does not prove a URL
// is one we trust. The action enforces a hard-coded hostname whitelist
// (Infura / Alchemy / QuickNode) after decryption. Because the action is
// content-addressed, changing that whitelist produces a new CID and must be
// re-authorised at the group level.
//
// js_params:
//   target               Contract whose view function is being read
//   callData             Hex-encoded calldata for the view function
//   sourceChainId        Chain id the read is performed on
//   registryAddress      Address of the ConsensusOracle that will accept the sig
//   registryChainId      Chain id that registry lives on
//   deadline             Unix seconds — signature is unusable after this
//   decryptPkpId         PKP that the RPC URLs were encrypted to
//   encryptedRpcUrls     Array of 3 ciphertexts, each one a full RPC URL
//   blockLagBlocks       Optional, defaults to 5 — how many blocks behind tip
//                        to read at, so all three providers have caught up
const ALLOWED_HOSTS = [
  /^[a-z0-9-]+\.infura\.io$/i,
  /^eth-[a-z0-9-]+\.g\.alchemy\.com$/i,
  /^[a-z0-9-]+\.g\.alchemy\.com$/i,
  /^[a-z0-9-]+\.quiknode\.pro$/i,
];

async function main({
  target,
  callData,
  sourceChainId,
  registryAddress,
  registryChainId,
  deadline,
  decryptPkpId,
  encryptedRpcUrls,
  blockLagBlocks = 5,
}) {
  if (!Array.isArray(encryptedRpcUrls) || encryptedRpcUrls.length !== 3) {
    return { authorized: false, reason: "exactly 3 RPC ciphertexts required" };
  }

  const rpcUrls = await Promise.all(
    encryptedRpcUrls.map((ciphertext) =>
      Lit.Actions.Decrypt({ pkpId: decryptPkpId, ciphertext })
    )
  );

  for (const url of rpcUrls) {
    let host;
    try {
      host = new URL(url).hostname;
    } catch {
      return { authorized: false, reason: "decrypted value is not a URL" };
    }
    if (!ALLOWED_HOSTS.some((re) => re.test(host))) {
      return { authorized: false, reason: `host not whitelisted: ${host}` };
    }
  }

  // Probe each provider for chain id + tip, in parallel.
  const probes = await Promise.all(
    rpcUrls.map((url) =>
      rpcBatch(url, [
        { method: "eth_chainId", params: [] },
        { method: "eth_blockNumber", params: [] },
      ])
    )
  );

  for (const [chainIdHex] of probes) {
    if (parseInt(chainIdHex, 16) !== sourceChainId) {
      return {
        authorized: false,
        reason: "one of the RPCs reported a different chain id",
      };
    }
  }
  const tip = Math.min(...probes.map(([, bn]) => parseInt(bn, 16)));
  const blockNumber = tip - blockLagBlocks;
  if (blockNumber <= 0) {
    return { authorized: false, reason: "chain too young for the lag setting" };
  }
  const blockTag = "0x" + blockNumber.toString(16);

  // Pull the call result + the block we read at, from each provider.
  const reads = await Promise.all(
    rpcUrls.map((url) =>
      rpcBatch(url, [
        { method: "eth_call", params: [{ to: target, data: callData }, blockTag] },
        { method: "eth_getBlockByNumber", params: [blockTag, false] },
      ])
    )
  );

  const returnDatas = reads.map(([result]) => result);
  const blockHashes = reads.map(([, block]) => block && block.hash);

  if (!returnDatas.every((r) => r === returnDatas[0])) {
    return {
      authorized: false,
      reason: "RPC consensus failed: return data disagreement",
      returnDatas,
      blockNumber,
    };
  }
  if (!blockHashes.every((h) => h && h === blockHashes[0])) {
    return {
      authorized: false,
      reason: "RPC consensus failed: block hash disagreement",
      blockHashes,
      blockNumber,
    };
  }

  const returnData = returnDatas[0];
  const observedAt = parseInt(reads[0][1].timestamp, 16);

  // Must match `keccak256(abi.encode(...))` in the contract.
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["address", "bytes", "bytes", "uint256", "uint256", "address", "uint256"],
      [target, callData, returnData, observedAt, deadline, registryAddress, registryChainId]
    )
  );

  // Action-identity signing: the private key is derived from THIS action's
  // CID. No other code path can produce a signature that recovers to the
  // same address.
  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const signature = await wallet.signMessage(ethers.utils.arrayify(digest));

  return {
    authorized: true,
    signature,
    signer: wallet.address,
    target,
    callData,
    returnData,
    observedAt,
    blockNumber,
    blockHash: blockHashes[0],
  };
}

async function rpcBatch(url, calls) {
  const body = calls.map((c, i) => ({ jsonrpc: "2.0", id: i + 1, ...c }));
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`RPC ${url} returned ${res.status}`);
  const arr = await res.json();
  arr.sort((a, b) => a.id - b.id);
  return arr.map((r) => {
    if (r.error) throw new Error(`RPC ${url} error: ${r.error.message}`);
    return r.result;
  });
}
