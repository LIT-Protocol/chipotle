// Lit Action: the Lit-side party of a threshold-ECDSA (DKLs23) signer. The
// example defaults to 2-of-3 (Lit + the user's hot share + a cold recovery
// share); the protocol here is t-of-n generic, so it also runs 2-of-2.
//
// THE WHOLE POINT: this action can never produce a signature on its own. The
// signing key is split between this action and the user. Each signature (and
// the key generation that precedes it) is an interactive MPC protocol; the
// full private key is never reconstructed anywhere — not even momentarily
// inside this V8 isolate. A compromised action cannot extract a signable key.
//
// The action is STATELESS across HTTP calls and the node has no storage, so we
// use the user as a transport for the action's own secret state: each round we
// serialize this party's MPC session, gzip it, and seal it with
// Lit.Actions.Encrypt({ pkpId }) — a ciphertext only THIS action CID can
// decrypt. The user holds that blob and sends it back for the next round. The
// long-lived "action keyshare" produced by DKG is sealed the same way.
//
// Protocol driving mirrors the DKLs23 reference flow (createFirstMessage +
// handleMessages rounds). secp256k1/ECDSA, so the output is a standard
// signature any EVM contract verifies with ecrecover.
//
// js_params (see client/mpcClient.js for the driver):
//   op           "keygen" | "sign"
//   round        1-based round index
//   sessionId    opaque string, fixed for the duration of one keygen/sign
//   pkpId        the PKP address used as the Encrypt/Decrypt boundary
//   encState     sealed session from the previous round (absent on round 1)
//   encKeyshare  sealed long-lived keyshare (sign round 1 only)
//   inMsgs       [{ p:<b64 payload>, f:<from_id>, t:<to_id|null> }] the user's
//                messages this round needs
//   commitments  [<b64>, <b64>] chain-code commitments [user, action] (keygen r4)
//   messageHash  <b64> 32-byte digest to sign. Sent in sign round 1, where it is
//                committed into the sealed state, and re-sent in round 4 where
//                it must match (nonce-reuse guard). Carried in the seal between.
//   participants, threshold   (keygen round 1; defaults to 3, 2 for 2-of-3 — or
//                              2, 2 for the --basic 2-of-2 variant)
//   chainPath    HD path for the SignSession (sign round 1; "m" = none)

// Import the DKLs23 wasm-bindgen glue from jsDelivr by EXPLICIT FILE PATH so
// the runtime serves it raw (no /+esm transform of wasm-bindgen output). We
// fetch the wasm bytes at runtime and feed them to initSync — no inlining.
// For production you would inline + pin the wasm so the action's CID commits
// to the exact bytes; see the README "Trust model" notes.
import {
  initSync,
  KeygenSession,
  SignSession,
  Keyshare,
  Message,
} from "@silencelaboratories/dkls-wasm-ll-web@1.2.0/dkls-wasm-ll-web.js";

const WASM_URL =
  "https://cdn.jsdelivr.net/npm/@silencelaboratories/dkls-wasm-ll-web@1.2.0/dkls-wasm-ll-web_bg.wasm";

let wasmReady = false;
async function ensureWasm() {
  if (wasmReady) return;
  const res = await fetch(WASM_URL);
  if (!res.ok) throw new Error(`fetch wasm ${res.status}`);
  initSync(new Uint8Array(await res.arrayBuffer()));
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

// --- Message <-> JSON -------------------------------------------------------
const msgToJson = (m) => ({ p: u8ToB64(m.payload), f: m.from_id, t: m.to_id ?? null });
const jsonToMsg = (j) => new Message(b64ToU8(j.p), j.f, j.t === null ? undefined : j.t);
const fromJsonList = (arr) => (arr || []).map(jsonToMsg);

// --- seal / unseal the action's secret state to its own CID -----------------
// Wrap = { kind, round, gz:<b64 of gzipped bytes>, ...bound }, gzipped+encrypted.
// `bound` carries integrity-checked metadata (sessionId, and for signing the
// committed messageHash) that the action verifies on the way back in.
// Lit.Actions.Encrypt is the only way to produce a ciphertext the action will
// accept, so the user can relay these blobs but cannot forge or alter the bound
// fields — that is what makes the sessionId / messageHash binding trustworthy.
async function seal(pkpId, bytes, kind, round, bound = {}) {
  const gz = await gzip(bytes);
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

async function main(params) {
  await ensureWasm();
  const { op } = params;
  if (op === "keygen") return keygenRound(params);
  if (op === "sign") return signRound(params);
  return { ok: false, error: `unknown op: ${op}` };
}

// ---------------------------------------------------------------------------
// Key generation — 5 rounds. The action is party 1; the user is party 0.
// Mirrors the DKLs reference dkg(): createFirstMessage -> 4x handleMessages
// (one carrying the chain-code commitments) -> keyshare().
// ---------------------------------------------------------------------------
async function keygenRound({ round, sessionId, pkpId, encState, inMsgs, commitments, participants, threshold }) {
  let session;
  let out = { ok: true, op: "keygen", round, sessionId };

  if (round === 1) {
    session = new KeygenSession(participants ?? 2, threshold ?? 2, 1);
    const m1a = session.createFirstMessage();
    out.outMsgs = [msgToJson(m1a)];
  } else {
    const { bytes, meta } = await unseal(pkpId, encState, "kg-state", round);
    if (meta.sessionId !== sessionId) {
      throw new Error("sealed-state sessionId mismatch — refusing cross-session splice");
    }
    session = KeygenSession.fromBytes(bytes);
    const incoming = fromJsonList(inMsgs);

    if (round === 2) {
      const m2a = session.handleMessages(incoming);
      const ccA = session.calculateChainCodeCommitment();
      out.outMsgs = m2a.map(msgToJson);
      out.ccA = u8ToB64(ccA); // user assembles [ccU, ccA] for round 4
    } else if (round === 3) {
      out.outMsgs = session.handleMessages(incoming).map(msgToJson);
    } else if (round === 4) {
      const cc = (commitments || []).map(b64ToU8);
      out.outMsgs = session.handleMessages(incoming, cc).map(msgToJson);
    } else if (round === 5) {
      session.handleMessages(incoming); // final, output unused
      const keyshare = session.keyshare(); // consumes session
      const pubU8 = keyshare.publicKey; // 33-byte compressed SEC1
      const pubHex = "0x" + u8ToHex(pubU8);
      out.encKeyshare = await seal(pkpId, keyshare.toBytes(), "keyshare");
      out.publicKey = pubHex;
      out.address = ethers.utils.computeAddress(pubHex);
      return out; // no further state to relay
    } else {
      return { ok: false, error: `bad keygen round ${round}` };
    }
  }

  out.encState = await seal(pkpId, session.toBytes(), "kg-state", round + 1, { sessionId });
  return out;
}

// ---------------------------------------------------------------------------
// Signing — 4 action rounds. Mirrors the DKLs reference dsg():
// createFirstMessage -> 3x handleMessages -> lastMessage(); the user does the
// final combine() locally to assemble [R, S].
// ---------------------------------------------------------------------------
async function signRound({ round, sessionId, pkpId, encState, encKeyshare, inMsgs, messageHash, chainPath }) {
  let session;
  // The message to sign is committed in round 1 and carried (integrity-checked)
  // in the sealed state through every round. The presignature/nonce is fixed
  // after rounds 1–3, so finalizing it for two different messages would reuse
  // the ECDSA nonce and leak the private key. Binding the hash here means a
  // replayed presignature can only ever produce a signature for the one hash it
  // was built for — the stateless action cannot otherwise detect the replay.
  let boundHash = messageHash;
  let out = { ok: true, op: "sign", round, sessionId };

  if (round === 1) {
    if (!messageHash) {
      throw new Error("sign round 1 requires messageHash (committed for the whole signing session)");
    }
    const { bytes } = await unseal(pkpId, encKeyshare, "keyshare");
    session = new SignSession(Keyshare.fromBytes(bytes), chainPath || "m");
    out.outMsgs = [msgToJson(session.createFirstMessage())];
  } else {
    const { bytes, meta } = await unseal(pkpId, encState, "sign-state", round);
    if (meta.sessionId !== sessionId) {
      throw new Error("sealed-state sessionId mismatch — refusing cross-session splice");
    }
    boundHash = meta.messageHash;
    session = SignSession.fromBytes(bytes);
    const incoming = fromJsonList(inMsgs);

    if (round === 2 || round === 3) {
      out.outMsgs = session.handleMessages(incoming).map(msgToJson);
    } else if (round === 4) {
      if (messageHash !== undefined && messageHash !== boundHash) {
        throw new Error("sign round 4 messageHash differs from the hash committed in round 1 — refusing (nonce-reuse guard)");
      }
      session.handleMessages(incoming); // final handleMessages, output unused
      const last = session.lastMessage(b64ToU8(boundHash));
      out.outMsgs = [msgToJson(last)];
      return out; // user combines; nothing more to relay
    } else {
      return { ok: false, error: `bad sign round ${round}` };
    }
  }

  out.encState = await seal(pkpId, session.toBytes(), "sign-state", round + 1, { sessionId, messageHash: boundHash });
  return out;
}

// Uint8Array -> hex (no 0x)
function u8ToHex(u8) {
  let h = "";
  for (let i = 0; i < u8.length; i++) h += u8[i].toString(16).padStart(2, "0");
  return h;
}
