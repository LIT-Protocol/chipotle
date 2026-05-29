// Lit Action: the Across fill policy gate for the Lit Solver Vault.
//
// This is the live-integration sibling of solverPolicy.js. Instead of a mock
// settlement contract, it reads a real Across deposit from the origin
// SpokePool and authorizes the vault to fill it on the destination chain.
//
// The design is deliberately authoritative: the action does NOT trust the bot
// to describe the fill. It looks up the `V3FundsDeposited` event for the given
// depositId on the origin chain and reconstructs the *entire* relay payload
// from what the deposit actually says — recipient, output token, output amount,
// deadlines, everything. The bot only chooses which depositId to fill and where
// to be repaid. There is no field for the bot to tamper with, so exfiltration
// isn't "rejected" so much as impossible by construction: the only relay the
// action will ever sign is the one that pays the deposit's real recipient.
//
// This matters because Across does NOT enforce deposit↔fill matching on the
// destination chain (it reconciles later, at reimbursement). On-chain, a
// compromised bot could fill to itself and just eat the loss. The thing that
// stops it is this policy.
//
// Policy checks, in order:
//   1. Both RPC hosts are whitelisted (trust anchor).
//   2. originChainId is on the vault's allowlist.
//   3. killSwitch on the vault is off.
//   4. the deposit's outputAmount is <= the vault's maxFillAmount.
//   5. the deposit exists and targets this vault's chain.
//
// (The deposit's own fillDeadline is enforced on-chain by the SpokePool when
// the fill is submitted, so the action doesn't need a clock for it.)
//
// js_params:
//   vaultAddress        AcrossSolverVault address (destination chain)
//   chainId             destination chain id (84532 Base Sepolia)
//   originSpokePool     SpokePool address on the origin chain
//   originChainId       origin chain id (11155111 Sepolia)
//   depositId           the deposit to fill
//   repaymentChainId    where the relayer wants to be reimbursed
//   authDeadline        unix seconds; the signed authorization is unusable
//                       after this. Supplied by the caller (the action has no
//                       reliable clock) and committed to in the signature, so
//                       the vault enforces it.
//   fromBlock           origin block to start the log search at (the deposit's
//                       block; keeps the eth_getLogs range small)
//   originRpcUrl        Alchemy Sepolia URL (host-whitelisted)
//   vaultRpcUrl         Alchemy Base-Sepolia URL (host-whitelisted)

const ALLOWED_ORIGIN_HOST = /^eth-sepolia\.g\.alchemy\.com$/i;
const ALLOWED_VAULT_HOST = /^base-sepolia\.g\.alchemy\.com$/i;

// Canonical Across SpokePool per origin chain. The caller passes
// originSpokePool, but it MUST match the pinned address for the claimed
// origin chain — otherwise a compromised usage key could point the action at
// an attacker contract that emits a forged FundsDeposited log, and the action
// would happily sign a relay reconstructed from that fake "deposit." The
// destination SpokePool does not verify the origin at fill time, so without
// this pin a fake deposit drains the vault. Adding a chain here changes the
// action's CID (and therefore its signer), which is the intended property.
const ALLOWED_ORIGIN_SPOKES = {
  "11155111": "0x5ef6C01E11889d86803e0B23e3cB3F9E9d97B662", // Sepolia
};

// The deployed Across SpokePools emit the bytes32-addressed `FundsDeposited`
// (uint256 depositId) — the SVM-compatible event, newer than the address-based
// `V3FundsDeposited` in the published deployment ABIs. We decode that and
// narrow the bytes32 fields back to EVM addresses for the (legacy, address-based)
// fillV3Relay the vault calls.
const depositIface = new ethers.utils.Interface([
  "event FundsDeposited(bytes32 inputToken, bytes32 outputToken, uint256 inputAmount, uint256 outputAmount, uint256 indexed destinationChainId, uint256 indexed depositId, uint32 quoteTimestamp, uint32 fillDeadline, uint32 exclusivityDeadline, bytes32 indexed depositor, bytes32 recipient, bytes32 exclusiveRelayer, bytes message)",
]);

// An EVM address padded into a bytes32 -> checksummed address. Rejects values
// whose high 12 bytes are non-zero: those are real (non-EVM / SVM) addresses,
// and silently truncating them to 20 bytes would sign a relay paying a
// different address than the deposit actually names.
function b32ToAddress(b32) {
  const clean = b32.toLowerCase().replace(/^0x/, "").padStart(64, "0");
  if (clean.slice(0, 24) !== "0".repeat(24)) {
    throw new Error(`bytes32 field is not an EVM address (non-zero high bytes): 0x${clean}`);
  }
  return ethers.utils.getAddress("0x" + clean.slice(24));
}

const vaultIface = new ethers.utils.Interface([
  "function killSwitch() view returns (bool)",
  "function maxFillAmount() view returns (uint256)",
  "function allowedOriginChain(uint256) view returns (bool)",
]);

// Tuple type for the relay payload — must match ISpokePool.V3RelayData in
// AcrossSolverVault.sol exactly, in both field order and types.
const RELAY_DATA_TUPLE =
  "tuple(address depositor,address recipient,address exclusiveRelayer,address inputToken,address outputToken,uint256 inputAmount,uint256 outputAmount,uint256 originChainId,uint32 depositId,uint32 fillDeadline,uint32 exclusivityDeadline,bytes message)";

async function main({
  vaultAddress,
  chainId,
  originSpokePool,
  originChainId,
  depositId,
  repaymentChainId,
  authDeadline,
  fromBlock,
  originRpcUrl,
  vaultRpcUrl,
}) {
  if (!authDeadline) {
    return { authorized: false, reason: "authDeadline is required" };
  }
  if (!hostOk(originRpcUrl, ALLOWED_ORIGIN_HOST)) {
    return { authorized: false, reason: "origin rpc host not whitelisted" };
  }
  if (!hostOk(vaultRpcUrl, ALLOWED_VAULT_HOST)) {
    return { authorized: false, reason: "vault rpc host not whitelisted" };
  }

  // Pin the origin SpokePool: the caller chooses originSpokePool, so it must
  // match the canonical address for the claimed chain or it could be an
  // attacker contract emitting forged deposits.
  const pinnedSpoke = ALLOWED_ORIGIN_SPOKES[String(originChainId)];
  if (!pinnedSpoke) {
    return { authorized: false, reason: `origin chain not supported: ${originChainId}` };
  }
  if (ethers.utils.getAddress(originSpokePool) !== ethers.utils.getAddress(pinnedSpoke)) {
    return {
      authorized: false,
      reason: `originSpokePool ${originSpokePool} is not the pinned SpokePool for chain ${originChainId}`,
    };
  }

  // All four reads are independent, so fire them concurrently rather than in
  // series: three eth_calls for the vault's policy config plus one eth_getLogs
  // for the deposit on the origin chain. This collapses ~4 sequential RPC
  // round-trips into ~1, which is the bulk of the authorization latency.
  const [killSwitch, allowed, maxFillAmount, deposit] = await Promise.all([
    readVault(vaultRpcUrl, vaultAddress, "killSwitch", []),
    readVault(vaultRpcUrl, vaultAddress, "allowedOriginChain", [originChainId]),
    readVault(vaultRpcUrl, vaultAddress, "maxFillAmount", []),
    findDeposit(originRpcUrl, originSpokePool, depositId, fromBlock),
  ]);

  if (killSwitch) {
    return { authorized: false, reason: "kill switch is engaged" };
  }
  if (!allowed) {
    return { authorized: false, reason: `origin chain not allowlisted: ${originChainId}` };
  }
  if (!deposit) {
    return { authorized: false, reason: `no FundsDeposited found for depositId ${depositId}` };
  }
  if (deposit.destinationChainId.toString() !== String(chainId)) {
    return {
      authorized: false,
      reason: `deposit targets chain ${deposit.destinationChainId} not this vault's chain ${chainId}`,
    };
  }

  const outputAmount = deposit.outputAmount;
  if (outputAmount.gt(maxFillAmount)) {
    return {
      authorized: false,
      reason: `outputAmount ${outputAmount.toString()} exceeds maxFillAmount ${maxFillAmount.toString()}`,
    };
  }
  // Never fill at a loss: the relayer pays outputAmount and is reimbursed at
  // most inputAmount. Without this, a compromised bot could create a deposit
  // with a tiny input and a huge output to itself and drain the vault on the
  // spread, all under the per-fill cap. A real solver also applies a fee floor
  // (output <= input * (1 - feeBps)); we keep the minimal invariant here.
  if (outputAmount.gt(deposit.inputAmount)) {
    return {
      authorized: false,
      reason: `outputAmount ${outputAmount.toString()} exceeds inputAmount ${deposit.inputAmount.toString()} (loss-making fill)`,
    };
  }

  // The canonical relay, built entirely from the on-chain deposit. bytes32
  // address fields are narrowed back to EVM addresses for the legacy struct.
  const relayData = {
    depositor: b32ToAddress(deposit.depositor),
    recipient: b32ToAddress(deposit.recipient),
    exclusiveRelayer: b32ToAddress(deposit.exclusiveRelayer),
    inputToken: b32ToAddress(deposit.inputToken),
    outputToken: b32ToAddress(deposit.outputToken),
    inputAmount: deposit.inputAmount.toString(),
    outputAmount: outputAmount.toString(),
    originChainId: String(originChainId),
    depositId: Number(depositId),
    fillDeadline: deposit.fillDeadline,
    exclusivityDeadline: deposit.exclusivityDeadline,
    message: deposit.message,
  };

  // --- sign (relayData, repaymentChainId, authDeadline, vault, chainId) ----
  // Must match AcrossSolverVault.executeAcrossFill's digest exactly.
  const encoded = ethers.utils.defaultAbiCoder.encode(
    [RELAY_DATA_TUPLE, "uint256", "uint256", "address", "uint256"],
    [
      [
        relayData.depositor,
        relayData.recipient,
        relayData.exclusiveRelayer,
        relayData.inputToken,
        relayData.outputToken,
        relayData.inputAmount,
        relayData.outputAmount,
        relayData.originChainId,
        relayData.depositId,
        relayData.fillDeadline,
        relayData.exclusivityDeadline,
        relayData.message,
      ],
      repaymentChainId,
      authDeadline,
      vaultAddress,
      chainId,
    ]
  );
  const digest = ethers.utils.keccak256(encoded);
  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const signature = await wallet.signMessage(ethers.utils.arrayify(digest));

  return {
    authorized: true,
    signature,
    signer: wallet.address,
    relayData,
    repaymentChainId,
    authDeadline,
    vaultAddress,
    chainId,
  };
}

function hostOk(url, re) {
  try {
    return re.test(new URL(url).hostname);
  } catch {
    return false;
  }
}

async function readVault(url, vault, fn, args) {
  const data = vaultIface.encodeFunctionData(fn, args);
  const result = await rpc(url, "eth_call", [{ to: vault, data }, "latest"]);
  if (!result || result === "0x") throw new Error(`${fn} -> empty (wrong vault/chain?)`);
  const decoded = vaultIface.decodeFunctionResult(fn, result);
  return decoded[0];
}

// Find the FundsDeposited log for a specific depositId on the origin chain.
async function findDeposit(url, spokePool, depositId, fromBlock) {
  const topic0 = depositIface.getEventTopic("FundsDeposited");
  const depositIdTopic = ethers.utils.hexZeroPad(
    ethers.BigNumber.from(depositId).toHexString(),
    32
  );
  // topics: [event, destinationChainId(any), depositId(specific)]
  const logs = await rpc(url, "eth_getLogs", [
    {
      address: spokePool,
      topics: [topic0, null, depositIdTopic],
      fromBlock: fromBlock ? ethers.utils.hexValue(fromBlock) : "earliest",
      toBlock: "latest",
    },
  ]);
  if (!logs || logs.length === 0) return null;
  const parsed = depositIface.parseLog(logs[logs.length - 1]);
  return parsed.args;
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
