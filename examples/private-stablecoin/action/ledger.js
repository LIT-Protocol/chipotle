// privUSD ledger action — the prover that replaces a ZK circuit.
//
// Runs across the Lit threshold network in TEEs. For each operation it reads
// chain state, decrypts/validates the relevant notes, enforces compliance,
// and signs the state update the PrivUSD contract will accept. The contract
// trusts exactly one key: this action's CID-derived signer
// (Lit.Actions.getLitActionPrivateKey()). Edit a byte here and the CID — and
// therefore the signer address — changes, so the deployed contract stops
// trusting the modified action.
//
// op = "mint" | "transfer" | "redeem". Dispatched in main().
//
// Compliance posture (see plans/private-stablecoin.md):
//   - OFAC sanctions screening runs on EVERY operation, on every recipient.
//   - KYC is required at the dollar edge: BOTH mint and redeem.
//
// DEMO-GRADE simplifications, all production-hardenable:
//   - `caller` is trusted from js_params to assert note ownership. Production
//     uses Lit.Auth.authSigAddress (the cryptographically authenticated
//     caller) instead.
//   - The KYC attestation is an EIP-191 signed message verified against a
//     `kycSigner` address from js_params. Production pins the KYC provider's
//     key via a hostname-anchored JWKS endpoint (same trust trick as the RPC
//     hosts below).
//   - Single OFAC provider. Production fans out to 2-3 (multi-source consensus).

const CHAINALYSIS_ORACLE = "0x40C57923924B5c5c5455c48D93317139ADDaC8fb";
const IS_SANCTIONED_SELECTOR = "0xdf592f7d"; // keccak256("isSanctioned(address)")[0..4]

// Trust anchor for sanctions screening: the data must come from Alchemy's
// Ethereum-mainnet endpoint (where the Chainalysis oracle lives). A
// caller-supplied chainId would be theater; the hostname is what TLS pins.
const ALLOWED_SCREENING_HOST = /^eth-mainnet\.g\.alchemy\.com$/i;

// Trust anchor for reading PrivUSD's own state (the commitment/nullifier
// checks in transfer/redeem). MUST be pinned: if a caller could supply this
// RPC, they could feed the action fabricated "this note exists and is unspent"
// answers and get a redeem signed against a note that was never minted —
// draining the real reserve. Same hostname-anchoring as the screening RPC. To
// change providers, edit this (which changes the action CID + signer).
const ALLOWED_CONTRACT_HOST = /^base-sepolia\.g\.alchemy\.com$/i;

// The ledger PKP that note contents are encrypted to. Baked in at setup
// (scripts/lib/buildAction.js) so it is bound into this action's CID — a
// caller cannot redirect encryption to a PKP they control, which would make
// the note undisclosable to a regulator (a sanctioned user could otherwise
// mint balances no warrant can ever open). Fails closed if left unconfigured.
const LEDGER_PKP_ID = "__LEDGER_PKP_ID__";

// ---------------------------------------------------------------------------
// Note crypto — MUST stay identical to scripts/lib/notes.js.
// ---------------------------------------------------------------------------
function commitmentOf(note) {
  return ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["address", "uint256", "bytes32"],
      [ethers.utils.getAddress(note.owner), String(note.amount), note.salt]
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

function sum(notes) {
  return notes.reduce((a, n) => a + BigInt(n.amount), 0n);
}

// ---------------------------------------------------------------------------
async function main(params) {
  // A real PKP address starts with 0x; the un-baked placeholder does not. (We
  // can't compare against the literal placeholder here — buildAction would
  // rewrite that occurrence too.)
  if (!LEDGER_PKP_ID.startsWith("0x")) {
    return { ok: false, reason: "action not configured: LEDGER_PKP_ID was not baked in at setup" };
  }
  try {
    switch (params.op) {
      case "mint":
        return await mint(params);
      case "transfer":
        return await transfer(params);
      case "redeem":
        return await redeem(params);
      default:
        return { ok: false, reason: `unknown op: ${params.op}` };
    }
  } catch (e) {
    return { ok: false, reason: `action error: ${e.message}` };
  }
}

// ---------------------------------------------------------------------------
// mint: USDC -> privUSD. KYC + OFAC gate the dollar edge.
// ---------------------------------------------------------------------------
async function mint(p) {
  const { depositor, depositAmount, outputs, kycAttestation, kycSigner } = p;

  const kyc = verifyKyc(kycAttestation, kycSigner, depositor);
  if (!kyc.ok) return kyc;

  const ofac = await screenAll([depositor, ...outputs.map((o) => o.owner)], p.screeningRpcUrl);
  if (!ofac.ok) return ofac;

  if (sum(outputs) !== BigInt(depositAmount)) {
    return { ok: false, reason: "depositAmount must equal sum of new notes" };
  }

  const { commitments, encryptedBlobs } = await buildNotes(outputs);

  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["string", "address", "uint256", "bytes32[]", "string[]", "bytes32", "uint256", "address", "uint256"],
      ["MINT", depositor, depositAmount, commitments, encryptedBlobs, p.nonce, p.deadline, p.contractAddress, p.chainId]
    )
  );

  return { ok: true, op: "mint", signature: await sign(digest), commitments, encryptedBlobs };
}

// ---------------------------------------------------------------------------
// transfer: private, value-preserving. OFAC on recipients; no KYC.
// ---------------------------------------------------------------------------
async function transfer(p) {
  const { inputs, outputs, caller } = p;

  const owned = checkOwnership(inputs, caller);
  if (!owned.ok) return owned;

  const live = await checkInputsLive(inputs, p.contractAddress, p.contractRpcUrl);
  if (!live.ok) return live;

  if (sum(inputs) !== sum(outputs)) {
    return { ok: false, reason: "sum(inputs) must equal sum(outputs)" };
  }

  const ofac = await screenAll(outputs.map((o) => o.owner), p.screeningRpcUrl);
  if (!ofac.ok) return ofac;

  const inputNullifiers = inputs.map(nullifierOf);
  const { commitments, encryptedBlobs } = await buildNotes(outputs);

  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["string", "bytes32[]", "bytes32[]", "string[]", "bytes32", "uint256", "address", "uint256"],
      ["TRANSFER", inputNullifiers, commitments, encryptedBlobs, p.nonce, p.deadline, p.contractAddress, p.chainId]
    )
  );

  return {
    ok: true,
    op: "transfer",
    signature: await sign(digest),
    inputNullifiers,
    outputCommitments: commitments,
    encryptedBlobs,
  };
}

// ---------------------------------------------------------------------------
// redeem: privUSD -> USDC. The dollar edge: KYC the redeemer + OFAC the payout.
// ---------------------------------------------------------------------------
async function redeem(p) {
  const { inputs, changeOutputs, withdrawAmount, recipient, caller, kycAttestation, kycSigner } = p;

  const owned = checkOwnership(inputs, caller);
  if (!owned.ok) return owned;

  // KYC the identity cashing out — redeem moves real dollars off the ledger.
  const kyc = verifyKyc(kycAttestation, kycSigner, caller);
  if (!kyc.ok) return kyc;

  const live = await checkInputsLive(inputs, p.contractAddress, p.contractRpcUrl);
  if (!live.ok) return live;

  const ofac = await screenAll([recipient, ...changeOutputs.map((o) => o.owner)], p.screeningRpcUrl);
  if (!ofac.ok) return ofac;

  if (BigInt(withdrawAmount) + sum(changeOutputs) !== sum(inputs)) {
    return { ok: false, reason: "withdrawAmount + change must equal sum(inputs)" };
  }

  const inputNullifiers = inputs.map(nullifierOf);
  const { commitments, encryptedBlobs } = await buildNotes(changeOutputs);

  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["string", "bytes32[]", "bytes32[]", "string[]", "uint256", "address", "bytes32", "uint256", "address", "uint256"],
      ["REDEEM", inputNullifiers, commitments, encryptedBlobs, withdrawAmount, recipient, p.nonce, p.deadline, p.contractAddress, p.chainId]
    )
  );

  return {
    ok: true,
    op: "redeem",
    signature: await sign(digest),
    inputNullifiers,
    changeCommitments: commitments,
    encryptedBlobs,
  };
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------
function checkOwnership(inputs, caller) {
  const c = ethers.utils.getAddress(caller);
  for (const n of inputs) {
    if (ethers.utils.getAddress(n.owner) !== c) {
      return { ok: false, reason: "caller does not own every input note" };
    }
  }
  return { ok: true };
}

// Read the contract's public mappings to confirm each input note exists and
// has not been spent. This is the action acting as prover: it validates
// against live chain state before authorizing the spend. The RPC host is
// pinned (see ALLOWED_CONTRACT_HOST) so the state it reads cannot be forged.
async function checkInputsLive(inputs, contract, rpcUrl) {
  let host;
  try {
    host = new URL(rpcUrl).hostname;
  } catch {
    return { ok: false, reason: "contractRpcUrl is not a valid URL" };
  }
  if (!ALLOWED_CONTRACT_HOST.test(host)) {
    return { ok: false, reason: `contract RPC host not whitelisted: ${host}` };
  }

  const COMMITMENTS_SEL = ethers.utils.id("commitments(bytes32)").slice(0, 10);
  const NULLIFIERS_SEL = ethers.utils.id("nullifiers(bytes32)").slice(0, 10);
  for (const n of inputs) {
    const c = commitmentOf(n);
    const exists = await ethCallBool(rpcUrl, contract, COMMITMENTS_SEL + c.slice(2));
    if (!exists) return { ok: false, reason: `input note not found on-chain: ${c}` };
    const spent = await ethCallBool(rpcUrl, contract, NULLIFIERS_SEL + nullifierOf(n).slice(2));
    if (spent) return { ok: false, reason: `input note already spent: ${c}` };
  }
  return { ok: true };
}

async function buildNotes(notes) {
  const commitments = [];
  const encryptedBlobs = [];
  for (const n of notes) {
    commitments.push(commitmentOf(n));
    encryptedBlobs.push(
      await Lit.Actions.Encrypt({ pkpId: LEDGER_PKP_ID, message: JSON.stringify(n) })
    );
  }
  return { commitments, encryptedBlobs };
}

function verifyKyc(attestation, kycSigner, subject) {
  // attestation = { message: JSON string, signature }. message carries
  // { subject, status, exp }. EIP-191 recover, then check binding + freshness.
  if (!attestation || typeof attestation.message !== "string" || !attestation.signature) {
    return { ok: false, reason: "KYC attestation missing" };
  }
  let claims;
  try {
    claims = JSON.parse(attestation.message);
  } catch {
    return { ok: false, reason: "KYC attestation message is not valid JSON" };
  }
  const recovered = ethers.utils.verifyMessage(attestation.message, attestation.signature);
  if (ethers.utils.getAddress(recovered) !== ethers.utils.getAddress(kycSigner)) {
    return { ok: false, reason: "KYC attestation not signed by the expected signer" };
  }
  if (claims.status !== "passed") return { ok: false, reason: "KYC status is not passed" };
  if (ethers.utils.getAddress(claims.subject) !== ethers.utils.getAddress(subject)) {
    return { ok: false, reason: "KYC attestation subject != expected subject" };
  }
  // Expiry is mandatory and must be numeric. A missing / string / null exp
  // must NOT silently mean "never expires".
  if (typeof claims.exp !== "number") {
    return { ok: false, reason: "KYC attestation has no numeric exp" };
  }
  if (Date.now() / 1000 > claims.exp) {
    return { ok: false, reason: "KYC attestation expired" };
  }
  return { ok: true };
}

async function screenAll(addresses, screeningRpcUrl) {
  let host;
  try {
    host = new URL(screeningRpcUrl).hostname;
  } catch {
    return { ok: false, reason: "screeningRpcUrl is not a valid URL" };
  }
  if (!ALLOWED_SCREENING_HOST.test(host)) {
    return { ok: false, reason: `screening RPC host not whitelisted: ${host}` };
  }
  for (const addr of [...new Set(addresses.map((a) => a.toLowerCase()))]) {
    const padded = addr.replace(/^0x/, "").padStart(64, "0");
    const result = await rpc(screeningRpcUrl, "eth_call", [
      { to: CHAINALYSIS_ORACLE, data: IS_SANCTIONED_SELECTOR + padded },
      "latest",
    ]);
    if (!result || result === "0x") {
      return { ok: false, reason: "Chainalysis oracle returned empty data — wrong chain?" };
    }
    if (BigInt(result) !== 0n) {
      return { ok: false, reason: `address on the Chainalysis sanctions oracle: ${addr}` };
    }
  }
  return { ok: true };
}

async function sign(digest) {
  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  return wallet.signMessage(ethers.utils.arrayify(digest));
}

async function ethCallBool(url, to, data) {
  const result = await rpc(url, "eth_call", [{ to, data }, "latest"]);
  return result && result !== "0x" && BigInt(result) !== 0n;
}

async function rpc(url, method, params) {
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const body = await res.json();
  if (body.error) throw new Error(`${method} -> ${body.error.message}`);
  return body.result;
}
