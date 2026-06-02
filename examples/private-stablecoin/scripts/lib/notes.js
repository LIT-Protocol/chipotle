// Note cryptography for privUSD — the client-side half.
//
// A "note" is a private unit of balance: { owner, amount, salt }. A wallet's
// balance is the sum of the notes it owns. Two derived values go on-chain:
//
//   commitment = keccak256(abi.encode(owner, amount, salt))
//       A hiding handle to the note. Reveals nothing.
//
//   nullifier  = keccak256(abi.encode("PRIVUSD_NULLIFIER", owner, salt))
//       Published when the note is spent, to prevent double-spend. Uses a
//       domain-separated preimage so it can't be confused with a commitment,
//       and is unlinkable to the commitment to an on-chain observer (both are
//       opaque hashes; only someone holding the secret `salt` can relate them).
//
// These formulas MUST stay byte-for-byte identical to the ones inlined in
// action/ledger.js — both sides recompute them and the contract trusts the
// action's results. If you change one, change all three.

const { ethers } = require("ethers");

function randomSalt() {
  return ethers.utils.hexlify(ethers.utils.randomBytes(32));
}

// amount is a string/BigNumber in USDC base units (6 decimals).
function makeNote(owner, amount) {
  return {
    owner: ethers.utils.getAddress(owner),
    amount: ethers.BigNumber.from(amount).toString(),
    salt: randomSalt(),
  };
}

function commitmentOf(note) {
  return ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["address", "uint256", "bytes32"],
      [ethers.utils.getAddress(note.owner), note.amount, note.salt]
    )
  );
}

function nullifierOf(note) {
  return ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["string", "address", "bytes32"],
      ["PRIVUSD_NULLIFIER", ethers.utils.getAddress(note.owner), note.salt]
    )
  );
}

function sumAmounts(notes) {
  return notes.reduce(
    (acc, n) => acc.add(ethers.BigNumber.from(n.amount)),
    ethers.BigNumber.from(0)
  );
}

// Spend-authorization digests. The note owner signs one of these (EIP-191) to
// prove they authorize a specific spend; the action recovers the signer and
// requires it to equal every input note's owner. This is what makes a note
// owned (not just "claimed") — knowing the opening is not enough to spend it.
// MUST stay identical to the copies inlined in action/ledger.js.
function transferAuthDigest({ inputNullifiers, outputCommitments, nonce, deadline, contractAddress, chainId }) {
  return ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["string", "bytes32[]", "bytes32[]", "bytes32", "uint256", "address", "uint256"],
      ["PRIVUSD_TRANSFER_AUTH", inputNullifiers, outputCommitments, nonce, deadline, contractAddress, chainId]
    )
  );
}

function redeemAuthDigest({ inputNullifiers, changeCommitments, withdrawAmount, recipient, nonce, deadline, contractAddress, chainId }) {
  return ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["string", "bytes32[]", "bytes32[]", "uint256", "address", "bytes32", "uint256", "address", "uint256"],
      ["PRIVUSD_REDEEM_AUTH", inputNullifiers, changeCommitments, withdrawAmount, recipient, nonce, deadline, contractAddress, chainId]
    )
  );
}

module.exports = {
  randomSalt, makeNote, commitmentOf, nullifierOf, sumAmounts,
  transferAuthDigest, redeemAuthDigest,
};
