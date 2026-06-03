// The USER-side party of the threshold-FROST (Ed25519) signer: the hot party
// (id 1), plus the cold recovery party (id 3) in the default 2-of-3.
//
// This runs locally on the user's machine and holds the user's signing share(s)
// in plaintext, never uploaded anywhere. It drives the interactive FROST
// protocol with the Lit Action (party id 2), routing protocol messages back and
// forth across /core/v1/lit_action. The action is stateless: it returns its own
// state sealed to its CID (encState / encActionKeyshare / encNonce), which we
// store and replay on the next round — we are just the transport for its secret
// state. Our own parties' state stays in memory for the duration of one op.
//
// Uses the SAME crypto as the action — the Node build of the lit-frost-wasm
// wrapper (../wasm/pkg-node) vs the action's Web build. The user's share is the
// long-lived secret persisted by store.js.
//
// Party ids: 1 = hot (user), 2 = Lit Action, 3 = cold (user, recovery).
// frost-dkg assigns ordinals by id order, so `allIds` must be identical and in
// the same order in every process (the action is told its own id explicitly).

const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const frost = require(path.join(__dirname, "..", "wasm", "pkg-node", "lit_frost_wasm.js"));

// The deployed action has the wasm glue inlined (mpcSigner.bundled.js, built by
// `npm run build:action`); fall back to the readable source otherwise. setup.js
// resolves the same way so the registered/sent CIDs match.
const _bundled = path.join(__dirname, "..", "action", "mpcSigner.bundled.js");
const ACTION_FILE = fs.existsSync(_bundled) ? _bundled : path.join(__dirname, "..", "action", "mpcSigner.js");
const ACTION_ID = 2; // the Lit Action's party id (hot=1, cold=3)

// --- byte helpers -----------------------------------------------------------
const u8 = (x) => (x instanceof Uint8Array ? x : Uint8Array.from(x));
const b64 = (x) => Buffer.from(u8(x)).toString("base64");
const unb64 = (s) => new Uint8Array(Buffer.from(s, "base64"));

// Gather the messages addressed to `me` from every other party's round output.
// `outs` maps senderId -> [{ dst, data }]; returns [{ from, data }].
function inbox(outs, me) {
  const msgs = [];
  for (const sender of Object.keys(outs)) {
    const sid = Number(sender);
    if (sid === me) continue;
    for (const w of outs[sender]) if (w.dst === me) msgs.push({ from: sid, data: w.data });
  }
  return msgs;
}

class MpcClient {
  constructor({ apiBase, usageApiKey, pkpId }) {
    this.apiBase = apiBase || "https://api.chipotle.litprotocol.com";
    this.usageApiKey = usageApiKey;
    this.pkpId = pkpId;
    this.code = fs.readFileSync(ACTION_FILE, "utf8");
  }

  async callAction(jsParams) {
    const res = await fetch(`${this.apiBase}/core/v1/lit_action`, {
      method: "POST",
      headers: { "X-Api-Key": this.usageApiKey, "Content-Type": "application/json" },
      body: JSON.stringify({ code: this.code, js_params: { pkpId: this.pkpId, ...jsParams } }),
    });
    const envelope = await res.json();
    if (envelope.has_error) {
      throw new Error(`Lit Action errored: ${envelope.logs || JSON.stringify(envelope)}`);
    }
    const out = envelope.response;
    if (!out || !out.ok) {
      throw new Error(`action returned: ${JSON.stringify(out || envelope)}`);
    }
    return out;
  }

  // -------------------------------------------------------------------------
  // Distributed key generation (FROST DKG, 3 rounds). The user holds the parties
  // in `userParties` (default [1, 3] = hot + cold for 2-of-3; [1] for 2-of-2).
  // The action is party `ACTION_ID` and runs in just two HTTP calls: round 1,
  // then round 2 + round 3 together (round 3 emits no messages and only needs
  // round-2 inputs, so it rides along in the second call — no extra round-trip).
  // Returns one signing share per user party, the action's sealed share, and the
  // group Ed25519 public key + Solana address.
  // -------------------------------------------------------------------------
  async keygen({ allIds = [1, 2, 3], threshold = 2, userParties = [1, 3], onRound } = {}) {
    if (userParties.includes(ACTION_ID)) throw new Error(`party ${ACTION_ID} is the Lit Action`);
    const sessionId = crypto.randomBytes(16).toString("hex");
    const ids = new Uint16Array(allIds);
    const state = {};

    // Round 1: every party broadcasts. User parties locally; action over HTTP.
    const out1 = {};
    for (const id of userParties) {
      const r = frost.dkg_round1(id, ids, threshold);
      state[id] = r.state;
      out1[id] = r.out;
    }
    onRound && onRound(1);
    const r1 = await this.callAction({ op: "dkg", round: 1, sessionId, myId: ACTION_ID, allIds, threshold });
    out1[ACTION_ID] = r1.out.map((w) => ({ dst: w.dst, data: unb64(w.data) }));

    // User parties run round 2 locally now (they have everyone's round-1 data).
    const out2 = {};
    for (const id of userParties) {
      const r = frost.dkg_round2(u8(state[id]), inbox(out1, id));
      state[id] = r.state;
      out2[id] = r.out;
    }

    // Round 2 + 3 for the action, in one call: send the round-1 messages it needs
    // (to run round 2) AND the round-2 messages addressed to it (to run round 3).
    onRound && onRound(2);
    const r2 = await this.callAction({
      op: "dkg",
      round: 2,
      sessionId,
      myId: ACTION_ID,
      allIds,
      threshold,
      encState: r1.encState,
      r1ToAction: inbox(out1, ACTION_ID).map((m) => ({ from: m.from, data: b64(m.data) })),
      r2ToAction: inbox(out2, ACTION_ID).map((m) => ({ from: m.from, data: b64(m.data) })),
    });
    out2[ACTION_ID] = r2.out.map((w) => ({ dst: w.dst, data: unb64(w.data) }));

    // User parties run round 3 (finalize).
    onRound && onRound(3);
    const userShares = {};
    let groupKey = null;
    for (const id of userParties) {
      const fin = frost.dkg_round3(u8(state[id]), inbox(out2, id));
      const pub = b64(fin.solana_pubkey);
      if (groupKey && pub !== groupKey) throw new Error(`party ${id} disagrees on the group key`);
      groupKey = pub;
      userShares[id] = b64(fin.signing_share);
    }

    // Every party (incl. the action) must agree on the same group key.
    if (groupKey !== r2.solanaPubkey) {
      throw new Error(`group key mismatch: user ${groupKey} vs action ${r2.solanaPubkey}`);
    }

    return {
      allIds,
      threshold,
      actionId: ACTION_ID,
      userShares, // { partyId: <b64 signing share> }
      encActionKeyshare: r2.encActionKeyshare,
      verifyingKey: r2.verifyingKey, // b64 lit-frost VerifyingKey (for sign/aggregate)
      solanaPubkey: r2.solanaPubkey, // b64 raw 32-byte Ed25519 pubkey = Solana address
    };
  }

  // -------------------------------------------------------------------------
  // Normal signing: the user's hot share (party 1) + the Lit Action (party 2).
  // FROST is 2 rounds; the user aggregates locally. Works for 2-of-2 and 2-of-3.
  // `message` is the raw bytes to sign (the Solana transaction message). Returns
  // the 64-byte Ed25519 signature (Buffer).
  // -------------------------------------------------------------------------
  async sign({ hotShare, encActionKeyshare, verifyingKey, threshold = 2, message, onRound }) {
    const sessionId = crypto.randomBytes(16).toString("hex");
    const hot = u8(Buffer.from(hotShare, "base64"));
    const vk = unb64(verifyingKey);
    const msg = u8(message);
    const msgB64 = b64(msg);

    // Round 1 (commit): user locally, action over HTTP. The action seals its
    // single-use nonce bound to this message (nonce-reuse guard).
    const uR1 = frost.sign_round1(hot);
    onRound && onRound(1);
    const aR1 = await this.callAction({
      op: "sign", round: 1, sessionId, myId: this.actionId ?? ACTION_ID,
      encActionKeyshare, message: msgB64,
    });
    const commitments = [
      { id: 1, data: uR1.commitment },
      { id: ACTION_ID, data: unb64(aR1.commitment) },
    ];
    const verifyingShares = [
      { id: 1, data: uR1.verifying_share },
      { id: ACTION_ID, data: unb64(aR1.verifyingShare) },
    ];

    // Round 2 (signature shares).
    const uR2 = frost.sign_round2(msg, 1, hot, vk, threshold, commitments, u8(uR1.nonce));
    onRound && onRound(2);
    const aR2 = await this.callAction({
      op: "sign", round: 2, sessionId, myId: ACTION_ID,
      encActionKeyshare, encNonce: aR1.encNonce, message: msgB64, threshold,
      verifyingKey, commitments: commitments.map((c) => ({ id: c.id, data: b64(c.data) })),
    });
    const signatureShares = [
      { id: 1, data: uR2.signature_share },
      { id: ACTION_ID, data: unb64(aR2.signatureShare) },
    ];

    const sig = frost.aggregate(msg, vk, commitments, signatureShares, verifyingShares);
    return Buffer.from(sig);
  }

  // -------------------------------------------------------------------------
  // RECOVERY signing: sign entirely client-side with two user-held shares
  // (hot + cold), with NO Lit Action involved — the self-custody escape hatch a
  // 2-of-3 buys you. No HTTP, no PKP, nothing leaves this machine.
  // `shares` is [{ bytes:<b64>, id }, { bytes, id }]. Returns the 64-byte sig.
  // -------------------------------------------------------------------------
  static signLocal({ shares, verifyingKey, threshold = 2, message }) {
    if (shares.length !== 2) throw new Error("recovery signing needs exactly two shares");
    const vk = unb64(verifyingKey);
    const msg = u8(message);

    const r1 = shares.map((s) => ({ id: s.id, share: u8(Buffer.from(s.bytes, "base64")), r: frost.sign_round1(u8(Buffer.from(s.bytes, "base64"))) }));
    const commitments = r1.map((x) => ({ id: x.id, data: x.r.commitment }));
    const verifyingShares = r1.map((x) => ({ id: x.id, data: x.r.verifying_share }));
    const signatureShares = r1.map((x) => ({
      id: x.id,
      data: frost.sign_round2(msg, x.id, x.share, vk, threshold, commitments, u8(x.r.nonce)).signature_share,
    }));

    const sig = frost.aggregate(msg, vk, commitments, signatureShares, verifyingShares);
    return Buffer.from(sig);
  }
}

module.exports = { MpcClient, ACTION_ID };
