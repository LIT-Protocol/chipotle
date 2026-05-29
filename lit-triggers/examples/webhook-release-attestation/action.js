// webhook-release-attestation — verify a GitHub release webhook, then anchor it
// on-chain with the keyless action wallet.
//
// Flow:
//   1. Verify GitHub's X-Hub-Signature-256 HMAC over the RAW request body
//      (params.event_raw) using a shared secret. A forged or unsigned request
//      is rejected before anything touches the chain.
//   2. Only act on `release` events with action `published`.
//   3. Write { repo, tag, commitish } to a ReleaseRegistry contract on-chain,
//      signed by this action's wallet — a key no human or server holds. The
//      registry becomes a tamper-evident, publicly verifiable record of what
//      the canonical release is.
//
// Config (default_params):
//   secret      — GitHub webhook secret (use Lit.Actions.Encrypt/Decrypt in prod)
//   rpcUrl      — destination chain RPC
//   registry    — ReleaseRegistry contract address
//   dryRun      — when true, sign the tx but do not broadcast (returns signedTx)

const header = (params, name) => {
  const h = params && params.headers && params.headers[name];
  return Array.isArray(h) ? h[0] : h || "";
};

// Constant-time string compare to avoid leaking the secret via timing.
const safeEqual = (a, b) => {
  if (typeof a !== "string" || typeof b !== "string" || a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
};

const main = async (params) => {
  // 1. Verify the signature over the exact raw bytes GitHub signed.
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
  const release = event.release || {};
  const repo = (event.repository && event.repository.full_name) || "";
  const tag = release.tag_name || "";
  const commitish = release.target_commitish || "";

  // 3. Build the on-chain attestation, signed by the keyless action wallet.
  const privateKey = await Lit.Actions.getLitActionPrivateKey();
  const wallet = new ethers.Wallet(privateKey);
  const iface = new ethers.utils.Interface([
    "function attest(string repo, string tag, string commitish)",
  ]);
  const data = iface.encodeFunctionData("attest", [repo, tag, commitish]);

  const provider = new ethers.providers.JsonRpcProvider(params.rpcUrl);
  const signer = wallet.connect(provider);
  // Explicit gasLimit so ethers skips eth_estimateGas — that call reverts for an
  // unfunded wallet, which would otherwise block the dryRun signing path.
  const txReq = {
    to: params.registry,
    data,
    gasLimit: ethers.BigNumber.from(params.gasLimit || "200000"),
  };

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
