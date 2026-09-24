// action-key-sign.js — a minimal Lit Action that signs "Chipotle Rocks!" with
// the ACTION's own CID-derived key (Lit.Actions.getLitActionPrivateKey).
//
// No PKP and no group permission are required for this key: it is derived by the
// TEE KMS from the action's IPFS CID. `ethers` is a runtime global (bundled), so
// this action makes no network calls. Used as the simpler of the two signing
// checks in 05-verify-pkp-sign.sh.
async function main() {
  const privateKey = await Lit.Actions.getLitActionPrivateKey();
  const wallet = new ethers.Wallet(privateKey);
  const signature = await wallet.signMessage("Chipotle Rocks!");
  return {
    signer_wallet_address: wallet.address,
    signature: signature,
    publicKey: wallet.publicKey,
  };
}
