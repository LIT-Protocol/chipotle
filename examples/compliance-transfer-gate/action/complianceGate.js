// Lit Action: screen `to` against the Chainalysis on-chain sanctions oracle
// (Ethereum mainnet) and sign a transfer authorization when the address is
// clear. The CompliantToken contract this signs for can live on *any* chain
// — that's the point of the example.
//
// Why Lit at all when Chainalysis is already on-chain? Because their oracle
// is only deployed on a handful of mainnets (Ethereum, Arbitrum, Polygon,
// BSC, Avalanche, Optimism, Celo). It is NOT on Base, Linea, Scroll, Sei,
// any L3, any testnet, or any non-EVM chain. On those chains a contract
// can't reach Chainalysis with a plain staticcall. This action reads the
// oracle on Ethereum mainnet, signs the attestation, and the signature is
// verifiable by a contract anywhere.
//
// The signature comes from Lit.Actions.getLitActionPrivateKey() — an
// identity derived from this action's IPFS CID. Edit the action by a byte
// and the address changes, so the on-chain CompliantToken stops trusting
// the modified action.
//
// js_params:
//   from                  msg.sender on the contract call
//   to                    recipient — the address being screened
//   amount                raw token units (string)
//   nonce                 32-byte hex string, replay-protection per (from, nonce)
//   deadline              unix seconds; signature is unusable after this
//   contractAddress       address of the CompliantToken
//   chainId               chain id where the CompliantToken is deployed
//   screeningRpcUrl       URL of an RPC for the chain where Chainalysis lives
//   screeningChainId      chain id of the screening chain — defensively checked
//                         against eth_chainId so a swapped RPC can't fake clean

const CHAINALYSIS_ORACLE = "0x40C57923924B5c5c5455c48D93317139ADDaC8fb";
// keccak256("isSanctioned(address)")[0..4]
const IS_SANCTIONED_SELECTOR = "0xdf592f7d";

async function main({
  from,
  to,
  amount,
  nonce,
  deadline,
  contractAddress,
  chainId,
  screeningRpcUrl,
  screeningChainId,
}) {
  // Defense in depth: verify the RPC is actually reporting the chain we
  // expect. Without this an attacker who controls js_params could point us
  // at a fork or a chain where Chainalysis isn't deployed (and the call
  // would return empty bytes, which BigInt parses as 0 = "clean").
  const reportedChainId = parseInt(
    await rpc(screeningRpcUrl, "eth_chainId", []),
    16
  );
  if (reportedChainId !== screeningChainId) {
    return {
      authorized: false,
      reason: `screening RPC reported chain ${reportedChainId}, expected ${screeningChainId}`,
    };
  }

  // Encode isSanctioned(address) calldata: selector + zero-padded address.
  const padded = to.toLowerCase().replace(/^0x/, "").padStart(64, "0");
  const callData = IS_SANCTIONED_SELECTOR + padded;

  const result = await rpc(screeningRpcUrl, "eth_call", [
    { to: CHAINALYSIS_ORACLE, data: callData },
    "latest",
  ]);

  // A plain `eth_call` to a non-existent contract returns "0x", which
  // BigInt happily parses as 0 — i.e. "clean." Treat empty as a hard error.
  if (!result || result === "0x") {
    return {
      authorized: false,
      reason: "Chainalysis oracle returned empty data — wrong chain?",
    };
  }
  const isSanctioned = BigInt(result) !== 0n;
  if (isSanctioned) {
    return { authorized: false, reason: "Recipient is on the Chainalysis sanctions oracle" };
  }

  // Must match `keccak256(abi.encode(...))` in the contract.
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["address", "address", "uint256", "bytes32", "uint256", "address", "uint256"],
      [from, to, amount, nonce, deadline, contractAddress, chainId]
    )
  );

  // Action-identity signing: derived from this action's CID.
  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const signature = await wallet.signMessage(ethers.utils.arrayify(digest));

  return {
    authorized: true,
    signature,
    signer: wallet.address,
    from,
    to,
    amount,
    nonce,
    deadline,
    contractAddress,
    chainId,
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
