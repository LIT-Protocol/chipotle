// lit-action-replay.js
//
// Authorization-tuple helpers (replay safety). Generate the nonce + deadline you
// fold into every signed digest, and reject stale authorizations before signing.

import { deny, bytesToHex } from "./lit-action-core.js";

/** Cryptographically random 32-byte nonce as a 0x-hex string (Web Crypto). */
export function newNonce() {
  const b = new Uint8Array(32);
  crypto.getRandomValues(b);
  return "0x" + bytesToHex(b);
}

/** Unix-seconds deadline `seconds` into the future. Fold into every digest. */
export function withDeadline(seconds) {
  return Math.floor(Date.now() / 1000) + seconds;
}

/** Reject a stale authorization before signing. Returns the seconds remaining. */
export function assertNotExpired({ deadline }) {
  const now = Math.floor(Date.now() / 1000);
  if (now > Number(deadline)) deny(`authorization expired ${now - deadline}s ago`);
  return Number(deadline) - now;
}
