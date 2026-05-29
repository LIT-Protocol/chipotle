// Lit Action: verify a GitHub `release` webhook, then anchor the release
// on-chain by sending an attestation transaction signed with this action's
// own wallet.
//
// Driven by a lit-triggers WEBHOOK trigger. Unlike a request/response Lit
// Action (where a caller submits the tx after the action signs), a trigger has
// no downstream caller — so this action BROADCASTS the transaction itself,
// from the wallet derived from its IPFS CID
// (Lit.Actions.getLitActionPrivateKey). That wallet is pinned as the
// ReleaseRegistry's `attester`, so only this exact action code can write to the
// registry. Edit the action by a byte and the CID, the signer address, and the
// registry's trust all change.
//
// Sender authentication: GitHub signs the webhook with HMAC-SHA256 over the raw
// body and sends it as X-Hub-Signature-256. lit-triggers passes the exact bytes
// as params.event_raw and the header through params.headers, so we can verify
// the request actually came from GitHub (with the shared secret) before
// touching the chain.
//
// default_params (set on the trigger):
//   secret    GitHub webhook secret (store via Lit.Actions.Encrypt in prod)
//   rpcUrl    destination chain RPC
//   registry  ReleaseRegistry address (its attester == this action's wallet)
//   gasLimit  optional; explicit so signing never depends on gas estimation
//   dryRun    when true, sign the tx but don't broadcast (returns signedTx)

const header = (params, name) => {
  const h = params && params.headers && params.headers[name];
  return Array.isArray(h) ? h[0] : h || "";
};

// Constant-time compare so a mismatch doesn't leak where via timing.
const safeEqual = (a, b) => {
  if (typeof a !== "string" || typeof b !== "string" || a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
};

const main = async (params) => {
  // 1. Verify GitHub's signature over the exact raw bytes it signed.
  const raw = (params && params.event_raw) || "";
  const provided = header(params, "x-hub-signature-256");
  const expected =
    "sha256=" +
    ethers.utils
      .computeHmac(
        ethers.utils.SupportedAlgorithm.sha256,
        ethers.utils.toUtf8Bytes(params.secret),
        ethers.utils.toUtf8Bytes(raw)
      )
      .slice(2);
  if (!safeEqual(provided, expected)) {
    return { ok: false, verified: false, error: "signature_mismatch" };
  }

  // 2. Only attest published releases.
  const event = (params && params.event) || {};
  const ghEvent = header(params, "x-github-event");
  if (ghEvent !== "release" || event.action !== "published") {
    return { ok: true, verified: true, skipped: `${ghEvent || "?"}/${event.action || "?"}` };
  }
  const repo = (event.repository && event.repository.full_name) || "";
  const tag = (event.release && event.release.tag_name) || "";
  const commitish = (event.release && event.release.target_commitish) || "";

  // 3. Build and (optionally) broadcast the on-chain attestation.
  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const iface = new ethers.utils.Interface([
    "function attest(string repo, string tag, string commitish)",
  ]);
  const data = iface.encodeFunctionData("attest", [repo, tag, commitish]);

  const provider = new ethers.providers.JsonRpcProvider(params.rpcUrl);
  const signer = wallet.connect(provider);
  // Explicit gasLimit so signing never depends on eth_estimateGas.
  const txReq = { to: params.registry, data, gasLimit: ethers.BigNumber.from(params.gasLimit || "200000") };

  if (params.dryRun) {
    const populated = await signer.populateTransaction(txReq);
    const signedTx = await signer.signTransaction(populated);
    return { ok: true, verified: true, signer: wallet.address, repo, tag, commitish, dryRun: true, signedTx };
  }
  const tx = await signer.sendTransaction(txReq);
  const receipt = await tx.wait();
  return {
    ok: true,
    verified: true,
    signer: wallet.address,
    repo,
    tag,
    commitish,
    txHash: receipt.transactionHash,
    block: receipt.blockNumber,
  };
};
