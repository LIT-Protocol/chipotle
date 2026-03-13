(async () => {
  const plaintext = await Lit.Actions.Decrypt({ pkpId, ciphertext });
  Lit.Actions.setResponse({ response: plaintext });
})();
