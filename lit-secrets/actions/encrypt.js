// lit-secrets encrypt action.
//
// Seals a secret value to the tenant's vault PKP inside the Chipotle TEE.
// Run by the lit-secrets control plane with the tenant's service usage key
// whenever a secret is created or rotated. Returns only the ciphertext.
const main = async ({ pkpId, value }) => {
  if (typeof pkpId !== 'string' || typeof value !== 'string') {
    throw new Error('pkpId and value are required');
  }
  const ciphertext = await Lit.Actions.Encrypt({ pkpId, message: value });
  return { ciphertext };
};
