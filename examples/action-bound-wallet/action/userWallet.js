// Lit Action: a wallet that is permanently bound to BOTH this exact code AND
// one specific user.
//
// The signing key is `Lit.Actions.getLitActionPrivateKey()` — a key derived
// deterministically from this action's IPFS CID. It only exists while this
// exact code runs inside the Lit network; there is no way to export it. So the
// wallet's key is bound to the code: edit a byte and the CID, key, and wallet
// address all change.
//
// The line that makes the wallet ALSO bound to a user is right below:
//
//     const OWNER_ADDRESS = "__OWNER_ADDRESS__";
//
// `OWNER_ADDRESS` is hardcoded, so it is part of the content that the CID
// hashes. Give two users two different addresses and you get two different
// source files, two different CIDs, and therefore two different derived
// wallets. That is the whole trick: **a unique, immutable wallet per user,
// with no contracts and no PKP minting** — you just stamp the user's address
// into the code. (scripts/_users.js does this stamping for the demo; in
// production you'd template this string per user when you upload the action.)
//
// What the action does:
//   action: "address"   — return the derived wallet address. No auth: anyone
//                          may learn where to deposit.
//   action: "withdraw"  — move ERC-20 out of the wallet, but ONLY if the
//                          request carries a signature from OWNER_ADDRESS over
//                          the exact (token, to, amount, nonce, deadline)
//                          tuple. The action verifies the signature, rebuilds
//                          and signs the ERC-20 transfer transaction with the
//                          wallet's key, and returns the raw signed tx for the
//                          caller to broadcast.
//
// Because the request is authorized by recovering the signer and comparing it
// to the hardcoded OWNER_ADDRESS, the usage key that runs the action does NOT
// grant spending power. Whoever holds the usage key can run anyone's action,
// but they can only ever read addresses or relay a withdrawal the real owner
// already signed. The spending authority lives with the owner's EOA key.

const OWNER_ADDRESS = "__OWNER_ADDRESS__";

// The canonical message the owner signs to authorize one withdrawal. The
// off-chain client (scripts/_canonical.js) builds the IDENTICAL string, so a
// signature only verifies for the precise withdrawal it describes. The wallet
// address and chainId pin it to this action on this chain; the nonce pins it
// to exactly one on-chain transaction; the deadline bounds its lifetime.
function withdrawalMessage({ wallet, chainId, token, to, amount, nonce, deadline }) {
  return [
    "Lit action-bound wallet — withdrawal authorization",
    `wallet:${wallet.toLowerCase()}`,
    `chainId:${chainId}`,
    `token:${token.toLowerCase()}`,
    `to:${to.toLowerCase()}`,
    `amount:${amount}`,
    `nonce:${nonce}`,
    `deadline:${deadline}`,
  ].join("\n");
}

// Trust anchor for the withdraw path. A caller can pass any rpcUrl, and the
// action reads the wallet's nonce + gas price from it. If that RPC were
// attacker-controlled it could (a) hand back a bogus gas price that the wallet
// then signs into the tx, burning the wallet's native gas on broadcast, and
// (b) return a future nonce so the signed tx lingers and executes later —
// weakening the "signed nonce == current on-chain nonce" guarantee. So we
// require the host to match a known node, anchored ^/$ so subdomain tricks
// like `sepolia.base.org.attacker.com` are rejected. Editing this regex
// changes the action's CID and therefore every derived wallet address — the
// same immutability property that binds the owner. Point it at your own
// provider (Alchemy/Infura) by editing the host here and `RPC_URL` to match.
const ALLOWED_RPC_HOST = /^sepolia\.base\.org$/i;

// Defense-in-depth cap on the gas price the action will sign over, so even a
// misbehaving trusted node can't make the wallet overpay. Base Sepolia gas is
// well under a gwei; 100 gwei is a generous ceiling.
const MAX_GAS_PRICE_WEI = ethers.utils.parseUnits("100", "gwei");

const erc20Iface = new ethers.utils.Interface([
  "function transfer(address to, uint256 amount) returns (bool)",
]);

async function main({ action, token, to, amount, nonce, deadline, signature, chainId, rpcUrl }) {
  // The wallet's key is derived from THIS action's CID. Two users with two
  // different OWNER_ADDRESS values get two different CIDs and land here with
  // two different `wallet.address` values.
  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());

  if (action === "address") {
    return { walletAddress: wallet.address, owner: OWNER_ADDRESS };
  }

  if (action !== "withdraw") {
    return { ok: false, reason: `unknown action: ${action} (expected "address" or "withdraw")` };
  }

  // --- 1. trust anchor: the RPC host must be the pinned node ---------------
  let host;
  try {
    host = new URL(rpcUrl).hostname;
  } catch {
    return { ok: false, reason: "rpcUrl is not a valid URL" };
  }
  if (!ALLOWED_RPC_HOST.test(host)) {
    return { ok: false, reason: `rpc host not whitelisted: ${host}` };
  }

  // --- 2. liveness ---------------------------------------------------------
  // NOTE: this bounds how long an authorization can be turned INTO a signed tx.
  // It does not expire the resulting tx — an EVM transaction has no deadline of
  // its own. Because we sign at the wallet's current nonce (below) the tx is
  // meant to be broadcast immediately; the deadline is the window for doing so.
  const now = Math.floor(Date.now() / 1000);
  if (typeof deadline !== "number" || now > deadline) {
    return { ok: false, reason: `authorization expired (now ${now} > deadline ${deadline})` };
  }

  // --- 3. on-chain nonce + gas price --------------------------------------
  // The nonce binds the owner's signature to exactly one transaction: once
  // this tx lands, the account nonce advances and the same signed message can
  // never produce another valid tx. We require the signed nonce to equal the
  // wallet's CURRENT nonce, so a stale (already-spent) authorization is dead.
  const [chainNonce, gasPrice, netChainId] = await Promise.all([
    rpcCall(rpcUrl, "eth_getTransactionCount", [wallet.address, "pending"]),
    rpcCall(rpcUrl, "eth_gasPrice", []),
    rpcCall(rpcUrl, "eth_chainId", []),
  ]);

  const currentNonce = Number(chainNonce);
  if (Number(netChainId) !== Number(chainId)) {
    return { ok: false, reason: `rpc chainId ${Number(netChainId)} != requested chainId ${chainId}` };
  }
  if (Number(nonce) !== currentNonce) {
    return { ok: false, reason: `stale nonce: signed for ${nonce}, wallet is at ${currentNonce}` };
  }
  if (ethers.BigNumber.from(gasPrice).gt(MAX_GAS_PRICE_WEI)) {
    return { ok: false, reason: `gas price ${gasPrice} exceeds cap ${MAX_GAS_PRICE_WEI.toString()}` };
  }

  // --- 4. verify the owner authorized THIS withdrawal ---------------------
  const message = withdrawalMessage({
    wallet: wallet.address,
    chainId,
    token,
    to,
    amount,
    nonce,
    deadline,
  });
  let recovered;
  try {
    recovered = ethers.utils.verifyMessage(message, signature);
  } catch (e) {
    return { ok: false, reason: `signature did not recover: ${e.message}` };
  }
  if (recovered.toLowerCase() !== OWNER_ADDRESS.toLowerCase()) {
    return {
      ok: false,
      reason: `unauthorized: signer ${recovered} is not the bound owner ${OWNER_ADDRESS}`,
    };
  }

  // --- 5. build + sign the ERC-20 transfer from the wallet ----------------
  // Recipient and amount come straight from the owner's signed tuple, so they
  // can never be redirected or changed here. The remaining caller-influenced
  // input is the RPC (nonce/gas), which is why the host is pinned above and the
  // gas price is capped.
  const data = erc20Iface.encodeFunctionData("transfer", [to, amount]);
  const tx = {
    to: token,
    data,
    value: 0,
    nonce: currentNonce,
    gasLimit: 100000, // a plain ERC-20 transfer fits comfortably under this
    gasPrice,
    chainId: Number(chainId),
  };
  const rawTx = await wallet.signTransaction(tx);

  return {
    ok: true,
    walletAddress: wallet.address,
    owner: OWNER_ADDRESS,
    token,
    to,
    amount,
    nonce: currentNonce,
    rawTx, // caller broadcasts this via eth_sendRawTransaction
  };
}

// Minimal JSON-RPC helper (the action runtime has fetch, not an ethers provider).
async function rpcCall(url, method, params) {
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const body = await res.json();
  if (body.error) throw new Error(`${method} -> ${body.error.message}`);
  return body.result;
}
