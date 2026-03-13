const go = async () => {
  const privateKey = await Lit.Actions.getLitActionPrivateKey();

  const wallet = new ethers.Wallet(privateKey);
  const signature = await wallet.signMessage("Chipotle Rocks!");
  const publicKey = wallet.publicKey;

  const data = {
    wallet_address: wallet.address,
    signature: signature,
    publicKey: publicKey,
  };

  Lit.Actions.setResponse({ response: data });
};
go();
