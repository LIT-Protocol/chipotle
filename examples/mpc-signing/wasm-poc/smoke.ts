// End-to-end de-risk for "DKLs23 2-of-2 ECDSA inside a stateless Lit Action".
//
// This mirrors what the real example will do:
//   1. The action's Deno/V8 runtime has no Node APIs. We instantiate the wasm
//      from BASE64-INLINED BYTES via initSync() — exactly how the action will
//      carry the ~642KB module in its source. No fetch, no file URL.
//   2. Party 0 = the user (stateful, on their machine).
//      Party 1 = the Lit Action (STATELESS). To prove the relay pattern, we
//      serialize party 1's session to bytes and rebuild it from bytes BETWEEN
//      EVERY ROUND — that round-trip is what will be Lit.Actions.Encrypt'd and
//      handed back to the user to resend next round.
//   3. Run full DKG (key generation) then DSG (signing).
//   4. Verify the [R,S] signature recovers to the secp256k1 public key the DKG
//      produced, and that the recovered address matches — i.e. plain EVM
//      ecrecover would accept it.

import { initSync, KeygenSession, Keyshare, SignSession, Message } from "./dkls-wasm-ll-web.js";
import { secp256k1 } from "npm:@noble/curves@1.4.0/secp256k1";
import { keccak_256 } from "npm:@noble/hashes@1.4.0/sha3";

// --- 1. Instantiate wasm from base64-inlined bytes (the action's mechanism) ---
const rawWasm = await Deno.readFile(new URL("./dkls-wasm-ll-web_bg.wasm", import.meta.url));
let bin = "";
for (let i = 0; i < rawWasm.length; i += 0x8000) bin += String.fromCharCode(...rawWasm.subarray(i, i + 0x8000));
const b64 = btoa(bin); // what we'd paste into the action
console.log(`wasm: ${rawWasm.length} bytes raw, ${b64.length} chars base64`);
const wasmBytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
initSync(wasmBytes);
console.log("✓ wasm instantiated in Deno via initSync(base64-decoded bytes)\n");

// message routing helpers (from the authors' test)
const others = (msgs: Message[], party: number) =>
  msgs.filter((m) => m.from_id != party).map((m) => m.clone());
const toParty = (msgs: Message[], party: number) =>
  msgs.filter((m) => m.to_id == party).map((m) => m.clone());

// Serialize→restore the ACTION party (party 1). This is the exact byte blob
// that gets Lit.Actions.Encrypt'd and relayed through the user each round.
let relayBytes = 0;
function relayKeygen(s: KeygenSession): KeygenSession {
  const b = s.toBytes();
  relayBytes = b.length;
  s.free();
  return KeygenSession.fromBytes(b);
}
function relaySign(s: SignSession): SignSession {
  const b = s.toBytes();
  relayBytes = b.length;
  s.free();
  return SignSession.fromBytes(b);
}

// --- 2 + 3a. DKG, round-tripping the action party every round ---
function dkg2of2(): { user: Keyshare; action: Keyshare } {
  let user = new KeygenSession(2, 2, 0); // party 0 = user
  let action = new KeygenSession(2, 2, 1); // party 1 = Lit Action (stateless)

  const m1u = user.createFirstMessage();
  let m1a = action.createFirstMessage();
  action = relayKeygen(action); // relay #1

  let m2u = user.handleMessages(others([m1u, m1a], 0));
  let m2a = action.handleMessages(others([m1u, m1a], 1));
  action = relayKeygen(action); // relay #2

  const ccU = user.calculateChainCodeCommitment();
  const ccA = action.calculateChainCodeCommitment();
  action = relayKeygen(action); // relay #3

  const all2 = [...m2u, ...m2a];
  let m3u = user.handleMessages(toParty(all2, 0));
  let m3a = action.handleMessages(toParty(all2, 1));
  action = relayKeygen(action); // relay #4

  const all3 = [...m3u, ...m3a];
  const commitments = [ccU, ccA];
  let m4u = user.handleMessages(toParty(all3, 0), commitments);
  let m4a = action.handleMessages(toParty(all3, 1), commitments);
  action = relayKeygen(action); // relay #5

  const all4 = [...m4u, ...m4a];
  user.handleMessages(others(all4, 0));
  action.handleMessages(others(all4, 1));

  return { user: user.keyshare(), action: action.keyshare() };
}

console.log("running 2-of-2 DKG (action party serialized/restored each round)...");
const { user: userShare, action: actionShare } = dkg2of2();
const pub = userShare.publicKey; // SEC1 (compressed 33B)
const pubHex = [...pub].map((b) => b.toString(16).padStart(2, "0")).join("");
console.log(`✓ DKG complete. relayed action-session blob = ${relayBytes} bytes`);
console.log(`  public key (${pub.length}B): ${pubHex}`);

// derive EVM address from the shared public key
const uncompressed = secp256k1.ProjectivePoint.fromHex(pubHex).toRawBytes(false); // 65B
const addr = "0x" + [...keccak_256(uncompressed.slice(1))].slice(12)
  .map((b) => b.toString(16).padStart(2, "0")).join("");
console.log(`  EVM address: ${addr}\n`);

// --- 3b. DSG (signing), round-tripping the action party every round ---
function sign2of2(uShare: Keyshare, aShare: Keyshare, msgHash: Uint8Array): { r: Uint8Array; s: Uint8Array } {
  let user = new SignSession(uShare, "m"); // party 0
  let action = new SignSession(aShare, "m"); // party 1 (stateless)

  const m1u = user.createFirstMessage();
  let m1a = action.createFirstMessage();
  action = relaySign(action); // relay #1

  const all1 = [m1u, m1a];
  let m2u = user.handleMessages(others(all1, 0));
  let m2a = action.handleMessages(others(all1, 1));
  action = relaySign(action); // relay #2

  const all2 = [...m2u, ...m2a];
  let m3u = user.handleMessages(toParty(all2, 0));
  let m3a = action.handleMessages(toParty(all2, 1));
  action = relaySign(action); // relay #3

  const all3 = [...m3u, ...m3a];
  user.handleMessages(toParty(all3, 0));
  action.handleMessages(toParty(all3, 1));
  action = relaySign(action); // relay #4

  const lu = user.lastMessage(msgHash);
  const la = action.lastMessage(msgHash);

  const sig = user.combine(others([lu, la], 0)); // [R, S]
  action.combine(others([lu, la], 1));
  return { r: sig[0], s: sig[1] };
}

const message = new TextEncoder().encode("Lit + user 2-of-2 MPC: authorize tx #1");
const msgHash = keccak_256(message);
console.log("running 2-of-2 signing (action party serialized/restored each round)...");
const { r, s } = sign2of2(userShare, actionShare, msgHash);
const rHex = [...r].map((b) => b.toString(16).padStart(2, "0")).join("");
const sHex = [...s].map((b) => b.toString(16).padStart(2, "0")).join("");
console.log(`✓ signing complete. relayed sign-session blob = ${relayBytes} bytes`);
console.log(`  r: ${rHex}\n  s: ${sHex}`);

// --- 4. Verify + recover (the ecrecover-compatibility proof) ---
const sigObj = new secp256k1.Signature(BigInt("0x" + rHex), BigInt("0x" + sHex));
const verified = secp256k1.verify(sigObj, msgHash, pub);
console.log(`\nsignature verifies against DKG public key: ${verified ? "✓ YES" : "✗ NO"}`);

let recovered = "(none)";
for (const v of [0, 1]) {
  try {
    const rp = sigObj.addRecoveryBit(v).recoverPublicKey(msgHash);
    const ua = rp.toRawBytes(false);
    const a = "0x" + [...keccak_256(ua.slice(1))].slice(12)
      .map((b) => b.toString(16).padStart(2, "0")).join("");
    if (a.toLowerCase() === addr.toLowerCase()) { recovered = `v=${27 + v}`; break; }
  } catch (_) { /* try next */ }
}
console.log(`ecrecover recovers the DKG address: ${recovered !== "(none)" ? "✓ YES (" + recovered + ")" : "✗ NO"}`);

if (verified && recovered !== "(none)") {
  console.log("\n🎉 ALL CHECKS PASSED — DKLs23 2-of-2 ECDSA runs in Deno from inlined wasm,");
  console.log("   survives per-round session serialization (the relay pattern),");
  console.log("   and produces a standard ecrecover-verifiable EVM signature.");
} else {
  console.log("\n❌ something failed");
  Deno.exit(1);
}
