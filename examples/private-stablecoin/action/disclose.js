// privUSD disclosure action — accountable, selective decryption.
//
// This is the capability no other private stablecoin can offer: a regulator
// holding a valid warrant, co-signed by a threshold of designated disclosure
// authorities, can decrypt ONE named note — while every other balance and
// transfer stays dark.
//
// js_params:
//   warrantMessage  the EXACT string the authorities signed. JSON of
//                   { noteCommitment, reason, expiry }. Passed verbatim (not
//                   re-serialized) so signature verification is byte-exact.
//   signatures      array of EIP-191 signatures over warrantMessage
//   authorities     the set of authorized disclosure-authority addresses
//   threshold       how many distinct authority signatures are required
//   encryptedBlob   the note ciphertext, read from the on-chain NoteCreated event
//
// Hardening vs. the obvious version:
//   - threshold is validated (positive integer, <= #authorities) so a caller
//     can't pass threshold=0 / "" / null and decrypt with zero signatures.
//   - the disclosed note is bound to the warrant: the decrypted note's
//     commitment must equal warrant.noteCommitment, so a warrant for note A
//     can't be used to open note B's blob.
//   - expiry is mandatory and numeric (a missing/string expiry is NOT
//     treated as "never expires").
//   - the ledger PKP is baked in (see scripts/lib/buildAction.js), not taken
//     from js_params, so a caller can't point decryption at a PKP they control.
//
// DEMO-GRADE: the authority set is still passed in js_params (production pins
// it on-chain or bakes it into this action's source), and the plaintext is
// returned over the response channel rather than re-encrypted to the
// regulator's pubkey. See plans/private-stablecoin.md.

const LEDGER_PKP_ID = "__LEDGER_PKP_ID__";

// Defense-in-depth floor: even if a caller supplies a tiny authority set, a
// disclosure needs at least this many co-signers. Production pins the exact
// authority set + threshold rather than trusting js_params.
const MIN_THRESHOLD = 3;

function commitmentOf(note) {
  return ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["address", "uint256", "bytes32"],
      [ethers.utils.getAddress(note.owner), String(note.amount), note.salt]
    )
  );
}

async function main({ warrantMessage, signatures, authorities, threshold, encryptedBlob }) {
  // A real PKP address starts with 0x; the un-baked placeholder does not. (We
  // can't compare against the literal placeholder here — buildAction would
  // rewrite that occurrence too.)
  if (!LEDGER_PKP_ID.startsWith("0x")) {
    return { ok: false, reason: "action not configured: LEDGER_PKP_ID was not baked in at setup" };
  }
  try {
    const gate = verifyWarrant(warrantMessage, signatures, authorities, threshold);
    if (!gate.ok) return gate;

    const plaintext = await Lit.Actions.Decrypt({ pkpId: LEDGER_PKP_ID, ciphertext: encryptedBlob });
    const note = JSON.parse(plaintext);

    // Bind the disclosure to the warrant: the blob must be the note the
    // warrant actually names.
    if (commitmentOf(note) !== gate.warrant.noteCommitment) {
      return { ok: false, reason: "decrypted note does not match warrant.noteCommitment" };
    }

    return {
      ok: true,
      warrantHash: ethers.utils.id(warrantMessage),
      disclosedNote: note,
      authoritiesUsed: gate.signers,
    };
  } catch (e) {
    return { ok: false, reason: `disclose error: ${e.message}` };
  }
}

function verifyWarrant(warrantMessage, signatures, authorities, threshold) {
  if (typeof warrantMessage !== "string") {
    return { ok: false, reason: "warrantMessage must be a string" };
  }
  let warrant;
  try {
    warrant = JSON.parse(warrantMessage);
  } catch {
    return { ok: false, reason: "warrantMessage is not valid JSON" };
  }
  if (!warrant.noteCommitment) {
    return { ok: false, reason: "warrant missing noteCommitment" };
  }
  if (typeof warrant.expiry !== "number") {
    return { ok: false, reason: "warrant has no numeric expiry" };
  }
  if (Date.now() / 1000 > warrant.expiry) {
    return { ok: false, reason: "warrant expired" };
  }

  if (!Array.isArray(authorities) || authorities.length === 0) {
    return { ok: false, reason: "no authorities provided" };
  }
  const authSet = new Set(authorities.map((a) => ethers.utils.getAddress(a)));

  // threshold must be a positive integer, at least MIN_THRESHOLD, and not
  // exceed the number of distinct authorities. Rejects 0 / negative / NaN /
  // "" / null / fractional.
  if (!Number.isInteger(threshold) || threshold < MIN_THRESHOLD) {
    return { ok: false, reason: `threshold must be an integer >= ${MIN_THRESHOLD}` };
  }
  if (threshold > authSet.size) {
    return { ok: false, reason: "threshold exceeds number of distinct authorities" };
  }

  const signers = new Set();
  for (const sig of signatures || []) {
    let recovered;
    try {
      recovered = ethers.utils.getAddress(ethers.utils.verifyMessage(warrantMessage, sig));
    } catch {
      continue; // malformed signature — ignore
    }
    if (authSet.has(recovered)) signers.add(recovered);
  }

  if (signers.size < threshold) {
    return {
      ok: false,
      reason: `warrant has ${signers.size} valid authority signatures, need ${threshold}`,
    };
  }
  return { ok: true, signers: [...signers], warrant };
}
