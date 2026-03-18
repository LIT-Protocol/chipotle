const main = async () => {
  const plaintext = await Lit.Actions.Decrypt({ pkpId, ciphertext });
  return plaintext;
};
