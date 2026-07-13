// lit-action-onchain.js
//
// On-chain read / write. View calls, receipt/log validation, native transfers
// from a PKP wallet, and the solver-vault recipient-binding pattern. Built on
// micro-eth-signer's ABI coder + Transaction and the hardened `rpc` helper.

import { addr, ethHex, weieth, Transaction } from "micro-eth-signer@0.19.0";
import { createContract } from "micro-eth-signer@0.19.0/advanced/abi.js";
import { deny, firstValue } from "./lit-action-core.js";
import { rpc } from "./lit-action-rpc.js";

// Minimal contracts reused by the gating module.
export const ERC20 = createContract([
  { type: "function", name: "balanceOf", inputs: [{ name: "o", type: "address" }], outputs: [{ type: "uint256" }] },
]);
export const ERC1155 = createContract([
  { type: "function", name: "balanceOf", inputs: [{ name: "o", type: "address" }, { name: "id", type: "uint256" }], outputs: [{ type: "uint256" }] },
]);
export const SANCTIONS = createContract([
  { type: "function", name: "isSanctioned", inputs: [{ name: "a", type: "address" }], outputs: [{ type: "bool" }] },
]);

function normalizeContractArgs(arg, inputs) {
  if (arg === undefined) return arg;
  if (!inputs) {
    if (Array.isArray(arg) && arg.length === 1) return arg[0];
    if (arg && typeof arg === "object" && !(arg instanceof Uint8Array)) {
      const keys = Object.keys(arg);
      if (keys.length === 1) return arg[keys[0]];
    }
    return arg;
  }
  const ins = inputs;
  if (ins.length === 0) return arg;
  if (ins.length === 1) {
    if (Array.isArray(arg)) return arg[0];
    if (arg && typeof arg === "object" && !(arg instanceof Uint8Array)) {
      const keys = Object.keys(arg);
      if (keys.length === 1) return arg[keys[0]];
    }
    return arg;
  }
  if (Array.isArray(arg) && ins.every((inp) => inp.name)) {
    return Object.fromEntries(ins.map((inp, i) => [inp.name, arg[i]]));
  }
  return arg;
}

/** eth_call a single method on a pre-built `createContract` instance. */
export async function ethCallContract(rpcUrl, address, contract, method, arg, inputs) {
  const data = ethHex.encode(contract[method].encodeInput(normalizeContractArgs(arg, inputs)));
  const result = await rpc(rpcUrl, "eth_call", [{ to: address, data }, "latest"]);
  if (!result || result === "0x") {
    throw new Error(`${method} -> empty result (wrong address or chain?)`);
  }
  return firstValue(contract[method].decodeOutput(ethHex.decode(result)));
}

/**
 * eth_call view helper. `abi` is JSON ABI fragments; `args` is either a named
 * object or a positional array matching `method`'s inputs.
 * @returns the decoded output (bare value for single-return functions).
 */
export async function readContract({ rpcUrl, address, abi, method, args }) {
  const fragment = abi.find((f) => f.type === "function" && f.name === method);
  if (!fragment) throw new Error(`readContract: method not in ABI: ${method}`);
  return ethCallContract(
    rpcUrl,
    address,
    createContract(abi),
    method,
    args,
    fragment.inputs || [],
  );
}

/**
 * Fetch a receipt, confirm it succeeded, and confirm the log at `logIndex` came
 * from `expectedContract` and carries `expectedTopic` as topics[0]. Returns the
 * matched log so the caller can decode topics/data.
 */
export async function readAndValidateLog({
  rpcUrl,
  txHash,
  logIndex,
  expectedContract,
  expectedTopic,
}) {
  const receipt = await rpc(rpcUrl, "eth_getTransactionReceipt", [txHash]);
  if (!receipt) deny(`no receipt for ${txHash} (unmined or unknown)`);
  if (receipt.status !== "0x1") deny(`tx ${txHash} did not succeed`);
  const log = (receipt.logs || []).find(
    (l) => Number(l.logIndex) === Number(logIndex),
  );
  if (!log) deny(`no log at index ${logIndex}`);
  if (addr.addChecksum(log.address) !== addr.addChecksum(expectedContract)) {
    deny(`log emitted by ${log.address}, expected ${expectedContract}`);
  }
  if ((log.topics[0] || "").toLowerCase() !== expectedTopic.toLowerCase()) {
    deny(`log topic ${log.topics[0]} != expected ${expectedTopic}`);
  }
  return log;
}

/**
 * Build/sign/broadcast a native ETH transfer from a PKP wallet (PKP must be
 * funded). Fees are derived from eth_gasPrice (maxFee = 2x gasPrice). Returns
 * the broadcast tx hash. EIP-1559 typed transaction via micro-eth-signer.
 */
export async function sendEth({ pkpId, to, amountEth, chainId, rpcUrl }) {
  const key = await Lit.Actions.getPrivateKey({ pkpId });
  const from = addr.fromPrivateKey(key);
  const [nonceHex, gasPriceHex] = await Promise.all([
    rpc(rpcUrl, "eth_getTransactionCount", [from, "pending"]),
    rpc(rpcUrl, "eth_gasPrice", []),
  ]);
  const gasPrice = BigInt(gasPriceHex);
  const tx = Transaction.prepare({
    to,
    value: weieth.decode(String(amountEth)),
    nonce: BigInt(nonceHex),
    maxPriorityFeePerGas: gasPrice,
    maxFeePerGas: gasPrice * 2n,
    gasLimit: 21000n,
    chainId: BigInt(chainId),
  });
  const signed = tx.signBy(key);
  const raw = signed.toHex();
  const txHash = await rpc(rpcUrl, "eth_sendRawTransaction", [raw]);
  return { txHash, from, to, amountEth };
}

/**
 * The solver-vault pattern. Reconstruct a fill from a PINNED settlement source
 * and require the caller-supplied recipient/token/amount to match what the
 * order actually says. Never trust the caller's claimed amounts.
 *
 * `orderAbi` defaults to a common `getOrder(bytes32) -> (recipient, token,
 * amount, exists)` shape; override for your settlement contract.
 */
export async function bindToOnchainOrder({
  rpcUrl,
  settlementContract,
  depositId,
  requested,
  orderAbi,
}) {
  const abi = orderAbi || [
    {
      type: "function",
      name: "getOrder",
      inputs: [{ name: "id", type: "bytes32" }],
      outputs: [
        { name: "recipient", type: "address" },
        { name: "token", type: "address" },
        { name: "amount", type: "uint256" },
        { name: "exists", type: "bool" },
      ],
    },
  ];
  const order = await readContract({
    rpcUrl,
    address: settlementContract,
    abi,
    method: "getOrder",
    args: { id: depositId },
  });
  if (!order.exists) deny(`no order for depositId ${depositId}`);
  if (addr.addChecksum(requested.recipient) !== addr.addChecksum(order.recipient)) {
    deny(`recipient ${requested.recipient} != order recipient ${order.recipient}`);
  }
  if (addr.addChecksum(requested.token) !== addr.addChecksum(order.token)) {
    deny(`token ${requested.token} != order token ${order.token}`);
  }
  if (BigInt(requested.amount) > BigInt(order.amount)) {
    deny(`amount ${requested.amount} exceeds order amount ${order.amount}`);
  }
  return order;
}
