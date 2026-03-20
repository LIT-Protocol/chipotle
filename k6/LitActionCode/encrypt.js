const main = async () => {
  const ciphertext = await Lit.Actions.Encrypt({ pkpId, message: challenge });
  return ciphertext;
};
