const main = async () => {
  const privateKey = await Lit.Actions.getLitActionPrivateKey();

  const wallet = new ethers.Wallet(privateKey);
  const signature = await wallet.signMessage("Chipotle Rocks!");
  const publicKey = wallet.publicKey;

  return {
    wallet_address: wallet.address,
    signature: signature,
    publicKey: publicKey,
  };
};
