(async () => {
  const ciphertext = await Lit.Actions.Encrypt({ pkpId, message: challenge });
  Lit.Actions.setResponse({ response: ciphertext });
})();
