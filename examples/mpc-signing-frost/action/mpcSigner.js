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
//   encState     sealed dkg state from round 1 (round 2 only)
//   r1ToAction   [{from,data(b64)}] round-1 messages addressed to the action (r2)
//   r2ToAction   [{from,data(b64)}] round-2 messages addressed to the action (r3)
//   --- sign ---
//   encActionKeyshare  sealed long-lived signing share
//   message      <b64> the raw message bytes to sign. Committed in sign round 1
//                (sealed into the nonce); round 2 refuses any other message
//                (FROST nonce reuse leaks the secret share — see README).
//   encNonce     sealed single-use signing nonce from round 1 (round 2 only)
//   verifyingKey <b64> group VerifyingKey (round 2)
//   commitments  [{id,data(b64)}] all signers' commitments (round 2)

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
  "https://cdn.jsdelivr.net/gh/clawdbot-glitch003/lit-frost-wasm@v0.0.1/lit_frost_wasm_bg.wasm";

let wasmReady = false;
async function ensureWasm() {
  if (wasmReady) return;
  const res = await fetch(WASM_URL);
  if (!res.ok) throw new Error(`fetch wasm ${res.status}`);
  await init(await res.arrayBuffer());
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
async function dkgRound({ round, sessionId, pkpId, myId, allIds, threshold, encState, r1ToAction, r2ToAction }) {
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
    out.encActionKeyshare = await seal(pkpId, fin.signing_share, "keyshare");
    out.verifyingKey = u8ToB64(arr(fin.verifying_key));
    out.verifyingShare = u8ToB64(arr(fin.verifying_share));
    out.solanaPubkey = u8ToB64(arr(fin.solana_pubkey));
    return out;
  }

  return { ok: false, error: `bad dkg round ${round}` };
}

// ---------------------------------------------------------------------------
// Signing — FROST, 2 rounds. Round 1 produces this party's nonce + commitment;
// the nonce is sealed AND bound to the committed message. Round 2 produces this
// party's signature share, but only for that same message — replaying the sealed
// nonce against a different message is refused (nonce reuse would leak the share).
// ---------------------------------------------------------------------------
async function signRound({ round, sessionId, pkpId, myId, encActionKeyshare, encNonce, message, threshold, verifyingKey, commitments }) {
  const out = { ok: true, op: "sign", round, sessionId };

  if (round === 1) {
    if (!message) throw new Error("sign round 1 requires the message (committed for the signing session)");
    const { bytes: share } = await unseal(pkpId, encActionKeyshare, "keyshare");
    const r = sign_round1(share);
    out.commitment = u8ToB64(arr(r.commitment));
    out.verifyingShare = u8ToB64(arr(r.verifying_share));
    // bind the message into the sealed single-use nonce
    out.encNonce = await seal(pkpId, r.nonce, "sign-nonce", 2, { sessionId, message });
    return out;
  }

  if (round === 2) {
    const { bytes: nonce, meta } = await unseal(pkpId, encNonce, "sign-nonce", 2);
    if (meta.sessionId !== sessionId) {
      throw new Error("sealed-nonce sessionId mismatch — refusing cross-session splice");
    }
    if (meta.message !== message) {
      throw new Error("sign round 2 message differs from the one committed in round 1 — refusing (nonce-reuse guard)");
    }
    const { bytes: share } = await unseal(pkpId, encActionKeyshare, "keyshare");
    const commits = (commitments || []).map((c) => ({ id: c.id, data: b64ToU8(c.data) }));
    const r = sign_round2(
      b64ToU8(message),
      myId,
      share,
      b64ToU8(verifyingKey),
      threshold,
      commits,
      nonce
    );
    out.signatureShare = u8ToB64(arr(r.signature_share));
    out.verifyingShare = u8ToB64(arr(r.verifying_share));
    return out;
  }

  return { ok: false, error: `bad sign round ${round}` };
}
