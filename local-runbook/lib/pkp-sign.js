// pkp-sign.js — a minimal Lit Action that signs "Chipotle Rocks!" with a MINTED
// PKP's key (not the action's own CID-derived key).
//
// `pkpId` arrives via js_params. The host authorizes Lit.Actions.getPrivateKey
// through the on-chain canUseWalletInAction(actionCid, pkpId) point query, which
// passes only because the action CID and the PKP are members of the same group
// (see 05-verify-pkp-sign.sh steps 5–7). `ethers` is a runtime global (bundled),
// so this action makes no network calls.
async function main({ pkpId }) {
  const privateKey = await Lit.Actions.getPrivateKey({ pkpId });
  const wallet = new ethers.Wallet(privateKey);
  const signature = await wallet.signMessage("Chipotle Rocks!");
  return {
    signer_wallet_address: wallet.address,
    signature: signature,
    publicKey: wallet.publicKey,
  };
}
