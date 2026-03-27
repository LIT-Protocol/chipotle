async function main() {
  const privateKey = await Lit.Actions.getLitActionPrivateKey();

  // Use native Rust ECDSA ops for ~100x faster key derivation and signing
  const { address, publicKey } = Lit.Actions.deriveEthAddress(privateKey);
  const signature = Lit.Actions.signMessage(privateKey, "Chipotle Rocks!");

  return {
    wallet_address: address,
    signature: signature,
    publicKey: publicKey,
  };
};
