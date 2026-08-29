// lit-secrets reader action (plaintext release tier).
//
// An agent calls this action directly on Chipotle with its own usage API key,
// passing a short-lived grant issued by the lit-secrets control plane. The
// grant is an EIP-191 signed JSON document from GRANT_SIGNER. This action
// verifies the grant, checks it matches the supplied ciphertext + vault PKP,
// decrypts inside the TEE, and returns the plaintext to the caller.
//
// The control plane never sees the plaintext: the only path from ciphertext
// to value is this code, executing in the TEE, on a vault PKP that is only
// usable by CIDs in the tenant's group.
const GRANT_SIGNER = '__GRANT_SIGNER__';

const main = async ({ grant, signature, ciphertext, pkpId }) => {
  if (
    typeof grant !== 'string' ||
    typeof signature !== 'string' ||
    typeof ciphertext !== 'string' ||
    typeof pkpId !== 'string'
  ) {
    throw new Error('grant, signature, ciphertext and pkpId are required');
  }

  const signer = ethers.utils.verifyMessage(grant, signature);
  if (signer.toLowerCase() !== GRANT_SIGNER.toLowerCase()) {
    throw new Error('grant signature invalid');
  }

  const g = JSON.parse(grant);
  const now = Math.floor(Date.now() / 1000);
  if (g.v !== 1) throw new Error('unsupported grant version');
  if (typeof g.exp !== 'number' || now > g.exp) throw new Error('grant expired');
  if (typeof g.iat !== 'number' || g.iat > now + 60) throw new Error('grant not yet valid');
  if (g.release !== 'plaintext') throw new Error('grant does not permit plaintext release');
  if (String(g.pkpId).toLowerCase() !== pkpId.toLowerCase()) {
    throw new Error('grant vault mismatch');
  }
  const ciphertextHash = ethers.utils.keccak256(ethers.utils.toUtf8Bytes(ciphertext));
  if (ciphertextHash.toLowerCase() !== String(g.ciphertextHash).toLowerCase()) {
    throw new Error('grant ciphertext mismatch');
  }

  const value = await Lit.Actions.Decrypt({ pkpId, ciphertext });
  return { name: g.name, version: g.version, value };
};
