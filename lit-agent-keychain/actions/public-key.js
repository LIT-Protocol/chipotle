// Public discovery only. This exact source is called directly through Chipotle.
async function main(params) {
  if (!/^Qm[1-9A-HJ-NP-Za-km-z]{44}$/.test(params?.cid || ""))
    throw new Error("Invalid action CID");
  return {
    ok: true,
    public_key: await Lit.Actions.getLitActionPublicKey({ ipfsId: params.cid }),
  };
}
