// Lit Action: the CoW Protocol settlement policy gate for the Lit Solver Vault.
//
// This is the CoW sibling of acrossPolicy.js. Instead of reconstructing a
// single Across relay from an on-chain deposit, it builds an entire CoW
// `settle(...)` batch from the trader's EIP-712-signed order and authorizes the
// vault to execute it.
//
// The design is the same as the rest of the family: the action does NOT trust
// the bot to describe the settlement. The bot supplies the trader's signed
// order (the intent), and the action:
//   - verifies the order signature against the settlement's EIP-712 domain,
//   - builds the *entire* settle calldata itself — token list, clearing prices,
//     the trade, and the two inventory interactions — from what the order
//     actually says (recipient, amounts, deadline),
//   - signs (keccak256(settleCalldata), pullToken, pullAmount, authDeadline,
//     vault, chainId).
//
// There is no field for the bot to tamper with: the receiver, the amounts and
// the clearing prices all come from the trader-signed order, and the
// interactions are fixed by the action. So exfiltration isn't "rejected" so
// much as impossible by construction — the only settlement the action will ever
// sign pays the order's real receiver and spends exactly the order's buy amount
// of the vault's inventory.
//
// This matters because CoW settlement is permissioned but *unconstrained*: the
// allowlisted solver can submit any batch the protocol accepts, including
// interactions that route the solver's own inventory to an attacker. The vault
// is the allowlisted solver here, and it only ever settles batches this action
// built. See contracts/CowSolverVault.sol.
//
// Policy checks, in order:
//   1. rpc host is whitelisted (trust anchor).
//   2. settlement target == the vault's pinned settlement (trust anchor).
//   3. killSwitch on the vault is off.
//   4. the order signature recovers to the claimed owner (EIP-712, sell order).
//   5. order.feeAmount == 0 and amounts are non-zero (keeps the batch exact).
//   6. order.buyAmount (the inventory spent) <= the vault's maxFillAmount.
//
// (order.validTo is enforced on-chain by the settlement when settle runs, so
// the action doesn't need a clock for it.)
//
// js_params:
//   vaultAddress   CowSolverVault address (Base Sepolia)
//   chainId        chain id (84532 Base Sepolia)
//   authDeadline   unix seconds; the signed authorization is unusable after
//                  this. Supplied by the caller (the action has no reliable
//                  clock) and committed to in the signature, so the vault
//                  enforces it.
//   order          the trader's signed order:
//                    { sellToken, buyToken, receiver, sellAmount, buyAmount,
//                      validTo, appData, feeAmount, owner, signature }
//                  kind=sell, partiallyFillable=false, balances=erc20 are
//                  fixed by this action (it builds the only batch shape it
//                  supports), so they are not taken from the caller.
//   rpcUrl         Alchemy Base-Sepolia URL (host-whitelisted)

const ALLOWED_RPC_HOST = /^base-sepolia\.g\.alchemy\.com$/i;

// CoW order EIP-712 type. The struct hash hashes `kind` and the balance fields
// as strings; we only ever build sell / erc20 / erc20 orders.
const ORDER_TYPE_STRING =
  "Order(address sellToken,address buyToken,address receiver,uint256 sellAmount,uint256 buyAmount,uint32 validTo,bytes32 appData,uint256 feeAmount,string kind,bool partiallyFillable,string sellTokenBalance,string buyTokenBalance)";

const vaultIface = new ethers.utils.Interface([
  "function settlement() view returns (address)",
  "function killSwitch() view returns (bool)",
  "function maxFillAmount() view returns (uint256)",
]);

// settle(...) ABI fragment, taken verbatim from @cowprotocol/contracts'
// GPv2Settlement artifact so the encoding matches the deployed contract exactly.
const SETTLE_FRAGMENT = {
  inputs: [
    { internalType: "contract IERC20[]", name: "tokens", type: "address[]" },
    { internalType: "uint256[]", name: "clearingPrices", type: "uint256[]" },
    {
      components: [
        { internalType: "uint256", name: "sellTokenIndex", type: "uint256" },
        { internalType: "uint256", name: "buyTokenIndex", type: "uint256" },
        { internalType: "address", name: "receiver", type: "address" },
        { internalType: "uint256", name: "sellAmount", type: "uint256" },
        { internalType: "uint256", name: "buyAmount", type: "uint256" },
        { internalType: "uint32", name: "validTo", type: "uint32" },
        { internalType: "bytes32", name: "appData", type: "bytes32" },
        { internalType: "uint256", name: "feeAmount", type: "uint256" },
        { internalType: "uint256", name: "flags", type: "uint256" },
        { internalType: "uint256", name: "executedAmount", type: "uint256" },
        { internalType: "bytes", name: "signature", type: "bytes" },
      ],
      internalType: "struct GPv2Trade.Data[]",
      name: "trades",
      type: "tuple[]",
    },
    {
      components: [
        { internalType: "address", name: "target", type: "address" },
        { internalType: "uint256", name: "value", type: "uint256" },
        { internalType: "bytes", name: "callData", type: "bytes" },
      ],
      internalType: "struct GPv2Interaction.Data[][3]",
      name: "interactions",
      type: "tuple[][3]",
    },
  ],
  name: "settle",
  outputs: [],
  stateMutability: "nonpayable",
  type: "function",
};
const settleIface = new ethers.utils.Interface([SETTLE_FRAGMENT]);

const erc20Iface = new ethers.utils.Interface([
  "function transfer(address to, uint256 amount) returns (bool)",
  "function transferFrom(address from, address to, uint256 amount) returns (bool)",
]);

async function main({ vaultAddress, chainId, authDeadline, order, rpcUrl }) {
  if (!authDeadline) {
    return { authorized: false, reason: "authDeadline is required" };
  }
  if (!order) {
    return { authorized: false, reason: "order is required" };
  }
  if (!hostOk(rpcUrl, ALLOWED_RPC_HOST)) {
    return { authorized: false, reason: "rpc host not whitelisted" };
  }

  // Kick off the policy-key derivation now so it overlaps the RPC latency below
  // instead of adding to it. (Attach a no-op catch so an abandoned promise on an
  // early-return path can't surface as an unhandled rejection.)
  const keyPromise = Lit.Actions.getLitActionPrivateKey();
  keyPromise.catch(() => {});

  // --- reads: the vault's pinned settlement + live policy config ----------
  // The settlement is the trust anchor: we read it from the vault (its
  // immutable), not from the caller, and build the batch for *that* contract.
  // All three are independent, so one concurrent round-trip — and we compute the
  // EIP-712 domain locally below rather than paying a second, dependent one.
  const [settlement, killSwitch, maxFillAmount] = await Promise.all([
    readContract(rpcUrl, vaultAddress, vaultIface, "settlement", []),
    readContract(rpcUrl, vaultAddress, vaultIface, "killSwitch", []),
    readContract(rpcUrl, vaultAddress, vaultIface, "maxFillAmount", []),
  ]);

  if (killSwitch) {
    return { authorized: false, reason: "kill switch is engaged" };
  }

  // The EIP-712 domain is fully determined by (chainId, settlement) plus CoW's
  // fixed name/version — so compute it locally instead of an extra RPC round-trip
  // for settlement.domainSeparator() (verified byte-for-byte against the deployed
  // contract). `settlement` came from the vault's immutable, so the domain is
  // still bound to the vault's pinned settlement.
  const domainSeparator = computeDomainSeparator(chainId, settlement);

  // --- validate the trader's order ----------------------------------------
  const sellAmount = ethers.BigNumber.from(order.sellAmount);
  const buyAmount = ethers.BigNumber.from(order.buyAmount);
  const feeAmount = ethers.BigNumber.from(order.feeAmount || "0");

  if (!feeAmount.isZero()) {
    // A non-zero fee is pulled from the trader on top of sellAmount; the batch
    // this action builds returns exactly sellAmount to the vault, so a fee
    // would be stranded. The CoW orderbook posts fee-less orders today; reject
    // anything else rather than silently leak it.
    return { authorized: false, reason: "order.feeAmount must be 0" };
  }
  if (sellAmount.isZero() || buyAmount.isZero()) {
    return { authorized: false, reason: "order amounts must be non-zero" };
  }
  if (buyAmount.gt(ethers.BigNumber.from(maxFillAmount))) {
    return {
      authorized: false,
      reason: `buyAmount ${buyAmount.toString()} exceeds maxFillAmount ${maxFillAmount.toString()}`,
    };
  }

  // Recover the EIP-712 signer and bind the order to its owner. The settlement
  // re-verifies this on-chain, but checking here lets us reject early and, more
  // importantly, means the receiver/amounts we build the batch from are exactly
  // the ones the owner signed — a tampered receiver changes the digest and the
  // recovery no longer matches the owner.
  const recovered = recoverOrderSigner(order, domainSeparator);
  if (recovered.toLowerCase() !== String(order.owner).toLowerCase()) {
    return {
      authorized: false,
      reason: `order signature recovers to ${recovered}, not owner ${order.owner}`,
    };
  }

  // --- build the canonical settlement -------------------------------------
  // tokens[0] = sellToken (what the trader gives up, ends up in the vault),
  // tokens[1] = buyToken  (what the vault pays out, from inventory).
  const tokens = [
    ethers.utils.getAddress(order.sellToken),
    ethers.utils.getAddress(order.buyToken),
  ];

  // Clearing prices fill the sell order exactly at its limit:
  //   executedBuy = sellAmount * price[sell] / price[buy]
  // With price[sell]=buyAmount and price[buy]=sellAmount, executedBuy=buyAmount
  // exactly (no surplus, no rounding), and the limit
  //   sellAmount*price[sell] >= buyAmount*price[buy]  holds with equality.
  const clearingPrices = [buyAmount.toString(), sellAmount.toString()];

  const trade = {
    sellTokenIndex: 0,
    buyTokenIndex: 1,
    receiver: ethers.utils.getAddress(order.receiver),
    sellAmount: sellAmount.toString(),
    buyAmount: buyAmount.toString(),
    validTo: Number(order.validTo),
    appData: order.appData,
    feeAmount: "0",
    flags: 0, // sell order, fill-or-kill, erc20/erc20 balances, EIP-712 signing
    executedAmount: 0, // ignored for fill-or-kill
    signature: order.signature,
  };

  // Inventory interactions, fixed by the action:
  //   pre  — the settlement pulls buyAmount of buyToken from the vault (the
  //          vault grants it a bounded allowance for exactly this), so the
  //          settlement can pay the receiver.
  //   post — the sellAmount of sellToken the settlement collected from the
  //          trader is forwarded to the vault (the asset the solver bought).
  const pre = [
    {
      target: tokens[1],
      value: 0,
      callData: erc20Iface.encodeFunctionData("transferFrom", [
        vaultAddress,
        settlement,
        buyAmount.toString(),
      ]),
    },
  ];
  const intra = [];
  const post = [
    {
      target: tokens[0],
      value: 0,
      callData: erc20Iface.encodeFunctionData("transfer", [
        vaultAddress,
        sellAmount.toString(),
      ]),
    },
  ];

  const settleCalldata = settleIface.encodeFunctionData("settle", [
    tokens,
    clearingPrices,
    [trade],
    [pre, intra, post],
  ]);

  const pullToken = tokens[1];
  const pullAmount = buyAmount.toString();

  // --- sign (keccak256(settleCalldata), pullToken, pullAmount, authDeadline,
  //           vault, chainId) — must match CowSolverVault.executeSettlement ---
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["bytes32", "address", "uint256", "uint256", "address", "uint256"],
      [
        ethers.utils.keccak256(settleCalldata),
        pullToken,
        pullAmount,
        authDeadline,
        vaultAddress,
        chainId,
      ]
    )
  );
  const wallet = new ethers.Wallet(await keyPromise);
  const signature = await wallet.signMessage(ethers.utils.arrayify(digest));

  return {
    authorized: true,
    signature,
    signer: wallet.address,
    settlement,
    settleCalldata,
    pullToken,
    pullAmount,
    authDeadline,
    vaultAddress,
    chainId,
    receiver: trade.receiver,
    sellToken: tokens[0],
    buyToken: tokens[1],
    sellAmount: sellAmount.toString(),
    buyAmount: buyAmount.toString(),
  };
}

function hostOk(url, re) {
  try {
    return re.test(new URL(url).hostname);
  } catch {
    return false;
  }
}

// GPv2's EIP-712 domain separator, computed locally. Matches what the deployed
// GPv2Settlement returns from domainSeparator(): the fixed CoW name/version with
// this chain id and the settlement as verifyingContract.
function computeDomainSeparator(chainId, settlement) {
  const DOMAIN_TYPE_HASH = ethers.utils.id(
    "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
  );
  return ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["bytes32", "bytes32", "bytes32", "uint256", "address"],
      [DOMAIN_TYPE_HASH, ethers.utils.id("Gnosis Protocol"), ethers.utils.id("v2"), chainId, settlement]
    )
  );
}

// EIP-712 digest = keccak256(0x1901 || domainSeparator || structHash), with the
// CoW order struct hash. recoverAddress gives the signer.
function recoverOrderSigner(order, domainSeparator) {
  const ORDER_TYPE_HASH = ethers.utils.id(ORDER_TYPE_STRING);
  const KIND_SELL = ethers.utils.id("sell");
  const BALANCE_ERC20 = ethers.utils.id("erc20");
  const structHash = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      [
        "bytes32",
        "address",
        "address",
        "address",
        "uint256",
        "uint256",
        "uint32",
        "bytes32",
        "uint256",
        "bytes32",
        "bool",
        "bytes32",
        "bytes32",
      ],
      [
        ORDER_TYPE_HASH,
        order.sellToken,
        order.buyToken,
        order.receiver,
        order.sellAmount,
        order.buyAmount,
        Number(order.validTo),
        order.appData,
        order.feeAmount || "0",
        KIND_SELL,
        false,
        BALANCE_ERC20,
        BALANCE_ERC20,
      ]
    )
  );
  const digest = ethers.utils.keccak256(
    ethers.utils.hexConcat(["0x1901", domainSeparator, structHash])
  );
  return ethers.utils.recoverAddress(digest, order.signature);
}

async function readContract(url, to, iface, fn, args) {
  const data = iface.encodeFunctionData(fn, args);
  const result = await rpc(url, "eth_call", [{ to, data }, "latest"]);
  if (!result || result === "0x") throw new Error(`${fn} -> empty (wrong address/chain?)`);
  return iface.decodeFunctionResult(fn, result)[0];
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
