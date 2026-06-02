// The canonical withdrawal-authorization message.
//
// This MUST stay byte-for-byte identical to `withdrawalMessage` inside
// action/userWallet.js. The owner signs the string this builds; the action
// rebuilds the same string from the js_params and verifies the signature
// against it. If the two ever drift, every signature fails to verify — which
// is the safe failure mode, but it means: edit one, edit both.

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

module.exports = { withdrawalMessage };
