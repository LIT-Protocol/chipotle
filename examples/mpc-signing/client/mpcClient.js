// The USER-side party of the 2-of-2 threshold-ECDSA signer (party 0).
//
// This runs locally on the user's machine and holds the user's keyshare
// (share_B) in plaintext, never uploaded anywhere. It drives the interactive
// MPC protocol with the Lit Action (party 1), routing protocol messages back
// and forth across /core/v1/lit_action. The action is stateless: it returns
// its own session sealed to its CID (encState / encKeyshare), which we store
// and replay on the next round — we are just the transport for its secret
// state, and it is the transport for ours is unnecessary because we keep ours
// in memory for the duration of one keygen/sign.
//
// Uses the Node build of the same DKLs23 library the action uses; the two
// builds are wire-compatible (identical Rust/serialization at the same
// version). The user's share is the long-lived secret persisted by store.js.

const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const dkls = require("@silencelaboratories/dkls-wasm-ll-node");

const { KeygenSession, SignSession, Keyshare, Message } = dkls;

const ACTION_FILE = path.join(__dirname, "..", "action", "mpcSigner.js");

// --- Message <-> JSON (matches the action's encoding) -----------------------
const msgToJson = (m) => ({ p: Buffer.from(m.payload).toString("base64"), f: m.from_id, t: m.to_id ?? null });
const jsonToMsg = (j) => new Message(Buffer.from(j.p, "base64"), j.f, j.t === null ? undefined : j.t);
const fromJsonList = (arr) => (arr || []).map(jsonToMsg);

// reference DKLs message routing
const others = (msgs, party) => msgs.filter((m) => m.from_id !== party).map((m) => m.clone());
const toParty = (msgs, party) => msgs.filter((m) => m.to_id === party).map((m) => m.clone());

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
  // Distributed key generation. 5 interactive rounds. The user may hold more
  // than one share: `userParties` lists the party ids this machine runs
  // (default [0] = 2-of-2; [0, 2] = 2-of-3 with a cold recovery share). The
  // Lit Action is always party 1. Returns one keyshare per user party, the
  // action's sealed keyshare, and the shared public key + EVM address.
  // -------------------------------------------------------------------------
  async keygen({ participants = 2, threshold = 2, userParties = [0], onRound } = {}) {
    const ACTION = 1;
    if (userParties.includes(ACTION)) throw new Error("party 1 is the Lit Action; the user cannot hold it");
    const sessionId = crypto.randomBytes(16).toString("hex");

    // one local KeygenSession per user-held party
    const u = new Map();
    for (const pid of userParties) u.set(pid, new KeygenSession(participants, threshold, pid));

    // Round 1: createFirstMessage (each party)
    const um1 = [...u.values()].map((s) => s.createFirstMessage());
    onRound && onRound(1);
    const r1 = await this.callAction({ op: "keygen", round: 1, sessionId, participants, threshold });
    let all1 = [...um1, ...fromJsonList(r1.outMsgs)];

    // Round 2: handleMessages(others) + each party's chain-code commitment
    const um2 = [];
    const cc = new Array(participants);
    for (const [pid, s] of u) {
      um2.push(...s.handleMessages(others(all1, pid)));
      cc[pid] = s.calculateChainCodeCommitment();
    }
    onRound && onRound(2);
    const r2 = await this.callAction({ op: "keygen", round: 2, sessionId, encState: r1.encState, inMsgs: others(all1, ACTION).map(msgToJson) });
    cc[ACTION] = Buffer.from(r2.ccA, "base64");
    let all2 = [...um2, ...fromJsonList(r2.outMsgs)];

    // Round 3: handleMessages(toParty)
    const um3 = [];
    for (const [pid, s] of u) um3.push(...s.handleMessages(toParty(all2, pid)));
    onRound && onRound(3);
    const r3 = await this.callAction({ op: "keygen", round: 3, sessionId, encState: r2.encState, inMsgs: toParty(all2, ACTION).map(msgToJson) });
    let all3 = [...um3, ...fromJsonList(r3.outMsgs)];

    // Round 4: handleMessages(toParty, commitments). Commitments are indexed
    // by party id [cc0, cc1, ..., cc_{n-1}] and must agree across all parties.
    const um4 = [];
    for (const [pid, s] of u) um4.push(...s.handleMessages(toParty(all3, pid), cc));
    onRound && onRound(4);
    const r4 = await this.callAction({
      op: "keygen", round: 4, sessionId,
      encState: r3.encState,
      inMsgs: toParty(all3, ACTION).map(msgToJson),
      commitments: cc.map((c) => Buffer.from(c).toString("base64")),
    });
    let all4 = [...um4, ...fromJsonList(r4.outMsgs)];

    // Round 5: final handleMessages, then keyshare() per party
    const shares = new Map();
    for (const [pid, s] of u) {
      s.handleMessages(others(all4, pid));
      shares.set(pid, s.keyshare());
    }
    onRound && onRound(5);
    const r5 = await this.callAction({ op: "keygen", round: 5, sessionId, encState: r4.encState, inMsgs: others(all4, ACTION).map(msgToJson) });

    // sanity: every party must agree on the same public key
    const userShares = {};
    let address = null;
    for (const [pid, ks] of shares) {
      const pub = "0x" + Buffer.from(ks.publicKey).toString("hex");
      if (pub.toLowerCase() !== r5.publicKey.toLowerCase()) {
        throw new Error(`public key mismatch: party ${pid} ${pub} vs action ${r5.publicKey}`);
      }
      userShares[pid] = Buffer.from(ks.toBytes()).toString("base64");
    }

    return {
      participants, threshold, actionParty: ACTION,
      userShares,                     // { partyId: <b64 keyshare> }
      encActionKeyshare: r5.encKeyshare,
      publicKey: r5.publicKey,
      address: r5.address,
      chainPath: "m",
    };
  }

  // -------------------------------------------------------------------------
  // Normal signing: the user's hot share (party 0) + the Lit Action (party 1).
  // 4 interactive rounds; the user does the final combine() locally. Works for
  // both 2-of-2 and 2-of-3 — {0, 1} is a valid quorum either way.
  // `hotShare` is the b64 keyshare for party 0. Returns { r, s } as 0x-hex.
  // -------------------------------------------------------------------------
  async sign({ hotShare, encActionKeyshare, chainPath = "m", messageHash, onRound }) {
    const sessionId = crypto.randomBytes(16).toString("hex");
    const user = new SignSession(Keyshare.fromBytes(Buffer.from(hotShare, "base64")), chainPath);
    const hashB64 = Buffer.from(messageHash).toString("base64");

    const s1u = user.createFirstMessage();
    onRound && onRound(1);
    const r1 = await this.callAction({ op: "sign", round: 1, sessionId, encKeyshare: encActionKeyshare, chainPath });
    const s1a = fromJsonList(r1.outMsgs);

    const s2u = user.handleMessages(others([s1u, ...s1a], 0));
    onRound && onRound(2);
    const r2 = await this.callAction({ op: "sign", round: 2, sessionId, encState: r1.encState, inMsgs: [s1u].map(msgToJson) });
    const s2a = fromJsonList(r2.outMsgs);

    const all2 = [...s2u, ...s2a];
    const s3u = user.handleMessages(toParty(all2, 0));
    onRound && onRound(3);
    const r3 = await this.callAction({ op: "sign", round: 3, sessionId, encState: r2.encState, inMsgs: toParty(all2, 1).map(msgToJson) });
    const s3a = fromJsonList(r3.outMsgs);

    const all3 = [...s3u, ...s3a];
    user.handleMessages(toParty(all3, 0)); // final handleMessages, output unused
    const s4u = user.lastMessage(messageHash);
    onRound && onRound(4);
    const r4 = await this.callAction({ op: "sign", round: 4, sessionId, encState: r3.encState, inMsgs: toParty(all3, 1).map(msgToJson), messageHash: hashB64 });
    const s4a = fromJsonList(r4.outMsgs);

    const sig = user.combine(others([s4u, ...s4a], 0)); // [R, S] as Uint8Arrays
    return {
      r: "0x" + Buffer.from(sig[0]).toString("hex"),
      s: "0x" + Buffer.from(sig[1]).toString("hex"),
    };
  }

  // -------------------------------------------------------------------------
  // RECOVERY signing: sign entirely client-side with two user-held shares
  // (hot + cold), with NO Lit Action involved. This is the self-custody escape
  // hatch a 2-of-3 buys you — if Lit ever disappears, the user still controls
  // the funds. No HTTP, no PKP, nothing leaves this machine.
  // `shares` is [{ bytes:<b64>, party:<id> }, { bytes, party }]. Returns {r,s}.
  // -------------------------------------------------------------------------
  static signLocal({ shares, messageHash, chainPath = "m" }) {
    if (shares.length !== 2) throw new Error("recovery signing needs exactly two shares");
    const [A, B] = shares;
    const a = new SignSession(Keyshare.fromBytes(Buffer.from(A.bytes, "base64")), chainPath);
    const b = new SignSession(Keyshare.fromBytes(Buffer.from(B.bytes, "base64")), chainPath);

    let m1 = [a.createFirstMessage(), b.createFirstMessage()];
    let m2 = [...a.handleMessages(others(m1, A.party)), ...b.handleMessages(others(m1, B.party))];
    let m3 = [...a.handleMessages(toParty(m2, A.party)), ...b.handleMessages(toParty(m2, B.party))];
    a.handleMessages(toParty(m3, A.party));
    b.handleMessages(toParty(m3, B.party));
    const la = a.lastMessage(messageHash);
    const lb = b.lastMessage(messageHash);
    const sig = a.combine(others([la, lb], A.party)); // [R, S]
    return {
      r: "0x" + Buffer.from(sig[0]).toString("hex"),
      s: "0x" + Buffer.from(sig[1]).toString("hex"),
    };
  }
}

module.exports = { MpcClient };
