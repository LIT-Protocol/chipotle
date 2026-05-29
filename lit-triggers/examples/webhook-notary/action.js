// webhook-notary — the minimal "sign with my keyless wallet" primitive.
//
// Takes any JSON webhook payload, computes a deterministic keccak256 digest of
// it, and signs the digest with this action's wallet. The private key is held
// by the Lit network (via Lit.Actions.getLitActionPrivateKey) — no server or
// human holds it — so the returned signature is a tamper-evident receipt that
// only this exact action code could have produced.
//
// Verify later with:
//   ethers.utils.verifyMessage(ethers.utils.arrayify(digest), signature) === signer
//
// This is the building block the other on-chain examples extend: notary + a
// destination contract = release attestation / subscription / oracle.

// Deterministic JSON serialization (sorted keys, recursive) so the digest is
// reproducible by any verifier that has the same payload.
const stableStringify = (v) => {
  if (Array.isArray(v)) return "[" + v.map(stableStringify).join(",") + "]";
  if (v && typeof v === "object") {
    return (
      "{" +
      Object.keys(v)
        .sort()
        .map((k) => JSON.stringify(k) + ":" + stableStringify(v[k]))
        .join(",") +
      "}"
    );
  }
  return JSON.stringify(v);
};

const main = async (params) => {
  const payload = (params && params.event) || {};
  const canonical = stableStringify(payload);
  const digest = ethers.utils.id(canonical); // keccak256(utf8 bytes)

  const privateKey = await Lit.Actions.getLitActionPrivateKey();
  const wallet = new ethers.Wallet(privateKey);
  const signature = await wallet.signMessage(ethers.utils.arrayify(digest));

  return {
    notarized_at: new Date().toISOString(),
    signer: wallet.address,
    canonical,
    digest,
    signature,
    payload,
  };
};
