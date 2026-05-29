// Lit Action: the solver fill policy gate for the Lit Solver Vault.
//
// A solver bot can ask this action to authorize a fill. The action enforces
// policy and only signs when the fill is legitimate. The signature it returns
// is the only thing that moves inventory out of the SolverVault, and the bot
// never holds the key that produces it — so a compromised bot can request
// fills, but it can't drain the vault.
//
// What "legitimate" means here, checked in order:
//   1. The RPC URL points at the whitelisted node (trust anchor — see below).
//   2. killSwitch on the vault is off.
//   3. The fill's settlement contract is on the vault's allowlist.
//   4. amount <= the vault's maxFillAmount.
//   5. The fill matches the ON-CHAIN order: same recipient, same token, and
//      amount no greater than the order's amount. This is the recipient-binding
//      check, and it's the kill shot for exfiltration — the bot can claim any
//      recipient it likes in js_params, but the order on-chain says who really
//      gets paid, and the bot can't rewrite that.
//
// Only after all five pass does the action sign
//   keccak256(abi.encode(token, recipient, amount, nonce, deadline, vault, chainId))
// with its CID-derived key — the exact tuple SolverVault.executeFill verifies.
//
// Why a CID-derived key (Lit.Actions.getLitActionPrivateKey) and not a
// long-lived PKP? Because it binds the *signer* to the *policy*. The vault
// pins this action's derived address as its policySigner. Edit a byte of this
// file and the CID — and therefore the signer address — changes, so the
// deployed vault stops honoring the modified policy automatically. The policy
// can't be swapped out from under the inventory.
//
// js_params:
//   vaultAddress        SolverVault address (also the signing domain)
//   chainId             chain id where the vault is deployed
//   token               ERC-20 being paid out
//   recipient           address the bot wants to pay (screened against order)
//   amount              raw token units (string)
//   nonce               32-byte hex string; replay protection per nonce
//   deadline            unix seconds; signature unusable after this
//   settlementContract  the intent/settlement contract this fill is for
//   depositId           32-byte order id within that settlement contract
//   rpcUrl              Alchemy Base-Sepolia URL (hostname-whitelisted below)

// The action's trust anchor. A caller can pass any rpcUrl in js_params, so we
// require the host to match Alchemy's Base-Sepolia endpoint — anchored ^/$ so
// subdomain tricks like `base-sepolia.g.alchemy.com.attacker.com` are rejected.
// Point the demo at a different node/chain by editing this regex (which mints a
// new action CID + signer address — you'll redeploy the vault with the new
// policySigner).
const ALLOWED_RPC_HOST = /^base-sepolia\.g\.alchemy\.com$/i;

const vaultIface = new ethers.utils.Interface([
  "function killSwitch() view returns (bool)",
  "function maxFillAmount() view returns (uint256)",
  "function allowedSettlement(address) view returns (bool)",
]);

const settlementIface = new ethers.utils.Interface([
  "function getOrder(bytes32) view returns (address recipient, address token, uint256 amount, bool exists)",
]);

async function main({
  vaultAddress,
  chainId,
  token,
  recipient,
  amount,
  nonce,
  deadline,
  settlementContract,
  depositId,
  rpcUrl,
}) {
  let host;
  try {
    host = new URL(rpcUrl).hostname;
  } catch {
    return { authorized: false, reason: "rpcUrl is not a valid URL" };
  }
  if (!ALLOWED_RPC_HOST.test(host)) {
    return {
      authorized: false,
      reason: `rpc host not whitelisted: ${host} (expected base-sepolia.g.alchemy.com)`,
    };
  }

  const amountBn = ethers.BigNumber.from(amount);

  // All four reads are independent, so fire them concurrently rather than in
  // series: three vault eth_calls (policy config) plus the settlement order
  // read. This collapses ~4 sequential RPC round-trips into ~1.
  const [killSwitch, allowed, maxFillAmount, order] = await Promise.all([
    ethCall(rpcUrl, vaultAddress, vaultIface, "killSwitch", []),
    ethCall(rpcUrl, vaultAddress, vaultIface, "allowedSettlement", [settlementContract]),
    ethCall(rpcUrl, vaultAddress, vaultIface, "maxFillAmount", []),
    ethCall(rpcUrl, settlementContract, settlementIface, "getOrder", [depositId]),
  ]);

  if (killSwitch) {
    return { authorized: false, reason: "kill switch is engaged" };
  }
  if (!allowed) {
    return {
      authorized: false,
      reason: `settlement contract not allowlisted: ${settlementContract}`,
    };
  }
  if (amountBn.gt(maxFillAmount)) {
    return {
      authorized: false,
      reason: `amount ${amountBn.toString()} exceeds maxFillAmount ${maxFillAmount.toString()}`,
    };
  }
  if (!order.exists) {
    return { authorized: false, reason: `no order for depositId ${depositId}` };
  }
  if (getAddress(recipient) !== getAddress(order.recipient)) {
    return {
      authorized: false,
      reason: `recipient ${recipient} does not match order recipient ${order.recipient}`,
    };
  }
  if (getAddress(token) !== getAddress(order.token)) {
    return {
      authorized: false,
      reason: `token ${token} does not match order token ${order.token}`,
    };
  }
  if (amountBn.gt(order.amount)) {
    return {
      authorized: false,
      reason: `amount ${amountBn.toString()} exceeds order amount ${order.amount.toString()}`,
    };
  }

  // --- all checks passed: sign the fill authorization ----------------------
  // Must match SolverVault.executeFill's digest exactly.
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["address", "address", "uint256", "bytes32", "uint256", "address", "uint256"],
      [token, recipient, amount, nonce, deadline, vaultAddress, chainId]
    )
  );

  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const signature = await wallet.signMessage(ethers.utils.arrayify(digest));

  return {
    authorized: true,
    signature,
    signer: wallet.address,
    vaultAddress,
    chainId,
    token,
    recipient,
    amount,
    nonce,
    deadline,
    depositId,
  };
}

function getAddress(a) {
  return ethers.utils.getAddress(a);
}

// eth_call to `to`, decoding the single-or-tuple result via `iface`.
async function ethCall(url, to, iface, fn, args) {
  const data = iface.encodeFunctionData(fn, args);
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "eth_call",
      params: [{ to, data }, "latest"],
    }),
  });
  const body = await res.json();
  if (body.error) throw new Error(`${fn} -> ${body.error.message}`);
  if (!body.result || body.result === "0x") {
    throw new Error(`${fn} -> empty result (wrong address or chain?)`);
  }
  const decoded = iface.decodeFunctionResult(fn, body.result);
  // Named tuples (getOrder) come back as an array with named props; single
  // returns (killSwitch, maxFillAmount, allowedSettlement) as a 1-element array.
  return decoded.length === 1 ? decoded[0] : decoded;
}
