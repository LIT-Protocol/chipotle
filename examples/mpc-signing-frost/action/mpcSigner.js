// Lit Action: the Lit-side party (id 2) of a threshold-FROST (Ed25519) signer.
// The example defaults to 2-of-3 (Lit + the user's hot share + a cold recovery
// share); the protocol is t-of-n generic, so it also runs 2-of-2.
//
// THE WHOLE POINT: this action can never produce a signature on its own. The
// signing key is split between this action and the user via a distributed key
// generation; the full private key is never reconstructed anywhere — not even
// momentarily inside this V8 isolate. A compromised action cannot extract a
// signable key.
//
// The action is STATELESS across HTTP calls and the node has no storage, so we
// use the user as transport for the action's own secret state: each round we
// serialize this party's state, gzip it, and seal it with
// Lit.Actions.Encrypt({ pkpId }) — a ciphertext only actions permitted in the
// group can decrypt. The user holds the blob and sends it back next round. The
// long-lived signing share produced by DKG is sealed the same way.
//
// Runs the SAME crypto as client/mpcClient.js — the Web build of the
// lit-frost-wasm wrapper (over lit-frost + frost-dkg). Output is a standard
// 64-byte Ed25519 signature any Solana validator verifies natively.
//
// js_params (see client/mpcClient.js for the driver):
//   op           "dkg" | "sign"
//   round        1 | 2
//   sessionId    opaque string, fixed for one dkg/sign
//   pkpId        the PKP address used as the Encrypt/Decrypt boundary
//   myId         this action's FROST party id (2)
//   --- dkg ---
//   allIds       [1,2,3] (or [1,2]) — the full participant id set, same order
//   threshold    signing threshold (2)
//   signPeers    ids the action may co-sign with online (round 2; [1]=hot). Sealed
//                into the keyshare; the cold share can't be used as cold+Lit online.
//   encState     sealed dkg state from round 1 (round 2 only)
//   r1ToAction   [{from,data(b64)}] round-1 messages addressed to the action (r2)
//   r2ToAction   [{from,data(b64)}] round-2 messages addressed to the action (r3)
//   --- sign (single, atomic round) ---
//   encActionKeyshare  sealed long-lived signing share (carries the group key,
//                      threshold, and this party's id — the action trusts THOSE,
//                      not caller-supplied values)
//   message      <b64> the raw message bytes to sign
//   peerCommitments  [{id,data(b64)}] the OTHER signer(s)' round-1 commitments
//
// Signing is one atomic call: the action generates its own single-use FROST
// nonce, signs, and discards the nonce — it is never sealed or relayed, so it
// can never be reused. (Reusing a FROST nonce across two transcripts is a
// secret-share-extraction oracle; doing both rounds in one stateless call
// removes the relay's ability to replay a nonce. See README "Trust model".)

// ── WASM LOADER ────────────────────────────────────────────────────────────
// This is the readable SOURCE. The DEPLOYED action is `mpcSigner.bundled.js`,
// produced by `npm run build:action`, which inlines the ~25 KB wasm-bindgen glue
// here (the Lit action bundler only resolves BARE NPM import specifiers, and the
// API gateway is too small to inline the 1.5 MB wasm) and keeps the runtime
// `fetch(WASM_URL)` below. Once `lit-frost-wasm` is published to npm, this import
// works as-is (like the ECDSA example imports DKLs) and `build:action` is moot.
import init, {
  dkg_round1,
  dkg_round2,
  dkg_round3,
  sign_round1,
  sign_round2,
} from "lit-frost-wasm@0.0.1/lit_frost_wasm.js"; // TODO: publish to npm

// Runtime fetch (a plain network call, not a bundler import — a full URL is OK).
// Currently a personal repo via jsDelivr; move to a Lit-owned package.
const WASM_URL =
  "https://cdn.jsdelivr.net/gh/clawdbot-glitch003/lit-frost-wasm@v0.0.2/lit_frost_wasm_bg.wasm";

// The action CID commits to THIS file, not to the fetched wasm. So we pin the
// wasm's SHA-256 here and refuse to run anything else: the CID transitively
// commits to the exact crypto bytes, even though they're fetched at runtime.
// (Update this when you rebuild the wasm; `npm run build:action` re-pins it.)
const WASM_SHA256 =
  "6b7eda8768478653f4cdfd85a6837f1de68ffc2b8ebb94d5ca335a98262951dc";

let wasmReady = false;
async function ensureWasm() {
  if (wasmReady) return;
  const res = await fetch(WASM_URL);
  if (!res.ok) throw new Error(`fetch wasm ${res.status}`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const hex = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
  if (hex !== WASM_SHA256) {
    throw new Error(`wasm hash mismatch: got ${hex}, expected ${WASM_SHA256} — refusing to run untrusted crypto`);
  }
  await init(bytes);
  wasmReady = true;
}

// --- base64 (Deno runtime: btoa/atob, chunked for large binary) -------------
function u8ToB64(u8) {
  let s = "";
  for (let i = 0; i < u8.length; i += 0x8000) {
    s += String.fromCharCode.apply(null, u8.subarray(i, i + 0x8000));
  }
  return btoa(s);
}
function b64ToU8(b64) {
  const bin = atob(b64);
  const u8 = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i);
  return u8;
}
const arr = (x) => (x instanceof Uint8Array ? x : Uint8Array.from(x)); // wasm returns number[]

// --- gzip via web CompressionStream (no Blob dependency) --------------------
async function streamThrough(u8, transform) {
  const input = new ReadableStream({
    start(c) {
      c.enqueue(u8);
      c.close();
    },
  });
  const ab = await new Response(input.pipeThrough(transform)).arrayBuffer();
  return new Uint8Array(ab);
}
const gzip = (u8) => streamThrough(u8, new CompressionStream("gzip"));
const gunzip = (u8) => streamThrough(u8, new DecompressionStream("gzip"));

// --- seal / unseal the action's secret state to its own CID -----------------
// Wrap = { kind, round, gz:<b64 of gzipped bytes>, ...bound }, gzipped+encrypted.
// `bound` carries integrity-checked metadata (sessionId, and for signing the
// committed message) the action verifies on the way back in. Lit.Actions.Encrypt
// is the only way to produce a ciphertext the action will accept, so the user
// can relay these blobs but cannot forge or alter the bound fields.
async function seal(pkpId, bytes, kind, round, bound = {}) {
  const gz = await gzip(arr(bytes));
  const wrapped = JSON.stringify({ kind, round, gz: u8ToB64(gz), ...bound });
  return await Lit.Actions.Encrypt({ pkpId, message: wrapped });
}
async function unseal(pkpId, ciphertext, expectKind, expectRound) {
  const w = JSON.parse(await Lit.Actions.Decrypt({ pkpId, ciphertext }));
  if (w.kind !== expectKind) {
    throw new Error(`sealed-state kind mismatch: got ${w.kind}, want ${expectKind}`);
  }
  if (expectRound !== undefined && w.round !== expectRound) {
    throw new Error(`sealed-state round mismatch: got ${w.round}, want ${expectRound}`);
  }
  return { bytes: await gunzip(b64ToU8(w.gz)), meta: w };
}

// relayed message lists <-> wasm shapes
const decodeMsgs = (list) => (list || []).map((m) => ({ from: m.from, data: b64ToU8(m.data) }));
const encodeOut = (out) => out.map((w) => ({ dst: w.dst, data: u8ToB64(arr(w.data)) }));

async function main(params) {
  await ensureWasm();
  const { op } = params;
  if (op === "dkg") return dkgRound(params);
  if (op === "sign") return signRound(params);
  return { ok: false, error: `unknown op: ${op}` };
}

// ---------------------------------------------------------------------------
// DKG — the action is one party across two HTTP calls. Round 1 emits the
// broadcast and seals state. Round 2 runs frost-dkg round 2 (given the round-1
// messages) AND round 3 (given the round-2 messages addressed to it), since
// round 3 needs no extra round-trip; it returns the action's round-2 messages
// plus its sealed long-lived signing share and the group key.
// ---------------------------------------------------------------------------
async function dkgRound({ round, sessionId, pkpId, myId, allIds, threshold, signPeers, encState, r1ToAction, r2ToAction }) {
  const out = { ok: true, op: "dkg", round, sessionId };

  if (round === 1) {
    const r = dkg_round1(myId, new Uint16Array(allIds), threshold);
    out.out = encodeOut(r.out);
    out.encState = await seal(pkpId, r.state, "dkg-state", 2, { sessionId });
    return out;
  }

  if (round === 2) {
    const { bytes, meta } = await unseal(pkpId, encState, "dkg-state", 2);
    if (meta.sessionId !== sessionId) {
      throw new Error("sealed-state sessionId mismatch — refusing cross-session splice");
    }
    const r2 = dkg_round2(bytes, decodeMsgs(r1ToAction));
    const fin = dkg_round3(arr(r2.state), decodeMsgs(r2ToAction));
    out.out = encodeOut(r2.out);
    const verifyingKey = u8ToB64(arr(fin.verifying_key));
    const solanaPubkey = u8ToB64(arr(fin.solana_pubkey));
    // Bind the protocol parameters into the sealed keyshare. Signing reads these
    // from the seal and ignores caller-supplied values, so a malicious relay
    // can't drive the action with a forged group key / threshold / party id, and
    // can't make the action co-sign with anyone but the allowed online peers
    // (signPeers = the hot share; the cold share is recovery-only, never online).
    out.encActionKeyshare = await seal(pkpId, fin.signing_share, "keyshare", undefined, {
      myId,
      threshold,
      verifyingKey,
      solanaPubkey,
      signPeers: signPeers || [],
    });
    out.verifyingKey = verifyingKey;
    out.verifyingShare = u8ToB64(arr(fin.verifying_share));
    out.solanaPubkey = solanaPubkey;
    return out;
  }

  return { ok: false, error: `bad dkg round ${round}` };
}

// ---------------------------------------------------------------------------
// Signing — ONE atomic FROST round. The action generates its own single-use
// nonce, commits, and signs in a single stateless call, then discards the nonce.
// Because the nonce never leaves this isolate (it is not sealed or relayed), a
// malicious relay cannot replay it against a second transcript — which is the
// only way to get two signature shares under one nonce and extract the share.
//
// All protocol parameters (group key, threshold, this party's id) come from the
// sealed keyshare, NOT from caller-supplied js_params, so the relay can't drive
// the action with forged values either.
// ---------------------------------------------------------------------------
async function signRound({ pkpId, encActionKeyshare, message, peerCommitments }) {
  if (!message) throw new Error("sign requires the message");
  const { bytes: share, meta } = await unseal(pkpId, encActionKeyshare, "keyshare");
  const myId = meta.myId;
  const threshold = meta.threshold;
  const verifyingKey = b64ToU8(meta.verifyingKey);
  if (myId === undefined || threshold === undefined || meta.verifyingKey === undefined || !Array.isArray(meta.signPeers)) {
    throw new Error("sealed keyshare is missing bound parameters — re-run keygen");
  }

  // Enforce the signing policy bound at keygen: the action only co-signs with the
  // allowed online peers (the hot share), exactly `threshold` signers total, no
  // duplicate or self ids. This keeps the cold share recovery-only and rejects
  // malformed/forged commitment sets before they reach the FROST library.
  const peers = peerCommitments || [];
  const seen = new Set();
  for (const c of peers) {
    if (c.id === myId) throw new Error("peer commitment claims the action's own id");
    if (!meta.signPeers.includes(c.id)) throw new Error(`party ${c.id} is not an allowed online co-signer`);
    if (seen.has(c.id)) throw new Error(`duplicate peer commitment for party ${c.id}`);
    seen.add(c.id);
  }
  if (peers.length + 1 !== threshold) {
    throw new Error(`expected ${threshold} signers (got ${peers.length + 1}) — wrong quorum`);
  }

  // Round 1: our own fresh nonce + commitment (in-memory only).
  const r1 = sign_round1(share);
  const myCommitment = arr(r1.commitment);

  // Full commitment set = peers + self, ordered by id (matches the user side).
  const commits = [
    ...peers.map((c) => ({ id: c.id, data: b64ToU8(c.data) })),
    { id: myId, data: myCommitment },
  ].sort((a, b) => a.id - b.id);

  // Round 2: our signature share for THIS exact transcript, then drop the nonce.
  const r2 = sign_round2(b64ToU8(message), myId, share, verifyingKey, threshold, commits, arr(r1.nonce));

  return {
    ok: true,
    op: "sign",
    commitment: u8ToB64(myCommitment),
    signatureShare: u8ToB64(arr(r2.signature_share)),
    verifyingShare: u8ToB64(arr(r2.verifying_share)),
  };
}
