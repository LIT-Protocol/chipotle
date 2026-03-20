async function main({ pkpId, ciphertext }) {
  const result = await Lit.Actions.Decrypt({ pkpId, ciphertext });
  return result;
}
