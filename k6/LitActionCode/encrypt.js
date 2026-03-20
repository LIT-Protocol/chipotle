async function main({ pkpId, challenge }) {
  const result = await Lit.Actions.Encrypt({ pkpId, message: challenge });
  return result;
}