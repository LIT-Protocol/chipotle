/**
 * Lit.Actions.Viem - custom account compatible with viem's toAccount interface.
 * Initialized by wallet address and PKP ID; signing uses Lit.Actions.sign from
 * 02_litActionsSDK.js with EcdsaK256Sha256. Exposed as Lit.Actions.Viem.toAccount().
 */
import { utils } from 'ext:lit_actions/00_ethers.js';

const SIGNING_SCHEME = 'EcdsaK256Sha256';
let sigCounter = 0;

function nextSigName() {
  return 'Lit.Viem.' + (sigCounter++) + '.' + Date.now();
}

/**
 * Sign raw digest bytes via Lit.Actions.sign (op_sign); signature shares are combined
 * by the Lit JS SDK client. Returns the op result (e.g. "success").
 * @param {Uint8Array|string} toSign - digest to sign (bytes or hex)
 * @param {string} pkpId - PKP ID
 * @returns {Promise<string>} result from Lit.Actions.sign
 */
async function signDigest(toSign, pkpId) {
  const bytes = typeof toSign === 'string' ? utils.arrayify(toSign) : new Uint8Array(toSign);
  const sigName = nextSigName();
  return globalThis.LitActions.sign({
    toSign: bytes,
    pkpId,
    sigName,
    signingScheme: SIGNING_SCHEME,
  });
}

/**
 * Create a custom account (viem toAccount-style) that signs with the Lit PKP via Lit.Actions.sign.
 * Initialization uses wallet address and PKP public key (no private key).
 * Signature shares are combined by the Lit JS SDK client; the combined signature is
 * available in the client response (by sigName).
 *
 * @name Lit.Actions.Viem.toAccount
 * @function toAccount
 * @param {Object} params
 * @param {string} params.address - EVM address of the account (0x...)
 * @param {string} params.pkpId - PKP ID used by the sign op
 * @returns {{ address: string, signMessage: Function, signTransaction: Function, signTypedData: Function }}
 */
function toAccount({ address, pkpId }) {
  const addr = typeof address === 'string' && address.startsWith('0x') ? address : '0x' + address;

  return {
    address: addr,

    async signMessage({ message }) {
      const hash = utils.hashMessage(message);
      const digestBytes = utils.arrayify(hash);
      return signDigest(digestBytes, pkpId);
    },

    async signTransaction(transaction, _opts = {}) {
      const payload = utils.serializeTransaction(transaction);
      const digest = utils.keccak256(payload);
      const digestBytes = utils.arrayify(digest);
      return signDigest(digestBytes, pkpId);
    },

    async signTypedData(typedData) {
      const { domain, types, message } = typedData;
      const hash = utils._TypedDataEncoder.hash(domain, types, message);
      const digestBytes = utils.arrayify(hash);
      return signDigest(digestBytes, pkpId);
    },
  };
}

(globalThis.LitActions = globalThis.LitActions || {}).Viem = { toAccount };
