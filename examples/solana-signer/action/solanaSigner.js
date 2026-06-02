// Lit Action: a keyless Solana wallet bound to this exact code.
//
// The wallet's ed25519 keypair is derived deterministically from this
// action's own identity key (Lit.Actions.getLitActionPrivateKey(), itself
// derived from the action's IPFS CID). The secp256k1 identity key is 32
// bytes, which is exactly an ed25519 seed — and Solana keypairs ARE ed25519
// keypairs derived from a 32-byte seed (this is what @solana/web3.js
// `Keypair.fromSeed` does). So the Solana address below is bound to the
// action: edit the code by a byte and the CID changes, the identity key
// changes, the seed changes, and the address changes. The private key never
// leaves the Lit TEE, so this exact code is the only thing that can ever
// sign for that Solana address — no key file to steal, no PKP to mint.
//
// Two operations, selected by `action`:
//   action: "address"  -> return the action's Solana address (base58)
//   action: "sign"     -> inspect a serialized legacy transaction message
//                         and, if it passes policy, ed25519-sign it
//
// The signer does NOT blindly sign whatever bytes it's handed. It parses the
// message and only signs a SINGLE SystemProgram transfer, whose fee payer is
// its own address, for no more than MAX_LAMPORTS. The canonical message bytes
// are built client-side by @solana/web3.js (see scripts/transfer.js); the
// parse here is read-only validation, so a parser quirk can only reject — it
// can never produce a signature over something other than the exact bytes the
// client will broadcast.
//
// js_params:
//   action     "address" | "sign"
//   message    (sign only) base64 of Transaction.serializeMessage()
//   recipient  (sign only) base58 recipient, cross-checked against the message
//
// Imports are pinned ESM from jsDelivr (see docs/lit-actions/imports.mdx).
// @noble/* and @scure/* are authored as pure ESM with no Node built-ins, so
// they run as-is inside the action runtime.
import * as ed from "@noble/ed25519@2.1.0";
import { sha512 } from "@noble/hashes@1.4.0/sha512/+esm";
import { base58 } from "@scure/base@1.1.6";

// @noble/ed25519 v2 needs a sha512 implementation wired in for its sync API.
ed.etc.sha512Sync = (...m) => sha512(ed.etc.concatBytes(...m));

// The most this wallet will ever sign away in one transfer. This is part of
// the hashed source, so changing it changes the CID and therefore the wallet
// address — the cap is bound to the address just like the key is. 0.5 SOL.
const MAX_LAMPORTS = 500_000_000n;

// SystemProgram's address is 32 zero bytes ("111...111" in base58).
const SYSTEM_PROGRAM_ID = new Uint8Array(32);
// SystemInstruction discriminant for Transfer is the u32 `2` (little-endian).
const TRANSFER_INSTRUCTION = 2;

async function main({ action, message, recipient }) {
  // Derive the ed25519 keypair from the action's identity key. The hex string
  // is a 32-byte secp256k1 private key; we reuse those 32 bytes as the ed25519
  // seed, exactly as Solana's Keypair.fromSeed does.
  const seed = ed.etc.hexToBytes(
    (await Lit.Actions.getLitActionPrivateKey()).replace(/^0x/, "")
  );
  const publicKey = ed.getPublicKey(seed);
  const address = base58.encode(publicKey);

  if (action === "address") {
    return { address };
  }

  if (action !== "sign") {
    return { authorized: false, reason: `unknown action "${action}"` };
  }

  // ---- Parse the legacy transaction message (read-only validation) --------
  let msg;
  try {
    msg = base64ToBytes(message);
  } catch {
    return { authorized: false, reason: "message is not valid base64" };
  }

  let parsed;
  try {
    parsed = parseLegacyMessage(msg);
  } catch (e) {
    return { authorized: false, reason: `could not parse message: ${e.message}` };
  }

  // Exactly one signer (the fee payer), and it must be us.
  if (parsed.numRequiredSignatures !== 1) {
    return {
      authorized: false,
      reason: `expected 1 required signature, got ${parsed.numRequiredSignatures}`,
    };
  }
  if (!bytesEqual(parsed.accountKeys[0], publicKey)) {
    return {
      authorized: false,
      reason: "fee payer (accountKeys[0]) is not this action's address",
    };
  }

  // Exactly one instruction, and it must be a SystemProgram transfer.
  if (parsed.instructions.length !== 1) {
    return {
      authorized: false,
      reason: `expected 1 instruction, got ${parsed.instructions.length}`,
    };
  }
  const ix = parsed.instructions[0];
  const programId = parsed.accountKeys[ix.programIdIndex];
  if (!programId || !bytesEqual(programId, SYSTEM_PROGRAM_ID)) {
    return { authorized: false, reason: "instruction is not a SystemProgram call" };
  }
  if (ix.data.length !== 12 || readU32LE(ix.data, 0) !== TRANSFER_INSTRUCTION) {
    return { authorized: false, reason: "instruction is not a SystemProgram transfer" };
  }

  // accounts = [fromIndex, toIndex]; the source must be the fee payer (us).
  if (ix.accounts.length !== 2 || ix.accounts[0] !== 0) {
    return { authorized: false, reason: "transfer does not debit the fee payer" };
  }
  const recipientKey = parsed.accountKeys[ix.accounts[1]];
  if (!recipientKey) {
    return { authorized: false, reason: "transfer recipient index out of range" };
  }
  const recipientAddress = base58.encode(recipientKey);
  // Cross-check against the caller-declared recipient so a mismatch surfaces
  // as a clear error rather than a silently-different signed transfer.
  if (recipient && recipientAddress !== recipient) {
    return {
      authorized: false,
      reason: `message pays ${recipientAddress}, not declared recipient ${recipient}`,
    };
  }

  const lamports = readU64LE(ix.data, 4);
  if (lamports > MAX_LAMPORTS) {
    return {
      authorized: false,
      reason: `transfer of ${lamports} lamports exceeds cap ${MAX_LAMPORTS}`,
    };
  }

  // ---- Sign the exact message bytes the client will broadcast -------------
  const signature = ed.sign(msg, seed);

  return {
    authorized: true,
    address,
    recipient: recipientAddress,
    lamports: lamports.toString(),
    // base64 of the 64-byte ed25519 signature, ready to attach to the tx.
    signature: bytesToBase64(signature),
  };
}

// ---------------------------------------------------------------------------
// Legacy (v0-less) Solana message parsing. Layout:
//   header: numRequiredSignatures, numReadonlySigned, numReadonlyUnsigned (u8)
//   accountKeys:     compact-u16 length, then N * 32 bytes
//   recentBlockhash: 32 bytes
//   instructions:    compact-u16 length, then for each:
//       programIdIndex (u8)
//       accounts: compact-u16 length, then that many u8 indices
//       data:     compact-u16 length, then that many bytes
// ---------------------------------------------------------------------------
function parseLegacyMessage(bytes) {
  let o = 0;
  const numRequiredSignatures = bytes[o++];
  const numReadonlySigned = bytes[o++];
  const numReadonlyUnsigned = bytes[o++];

  let numKeys;
  [numKeys, o] = readCompactU16(bytes, o);
  const accountKeys = [];
  for (let i = 0; i < numKeys; i++) {
    accountKeys.push(bytes.slice(o, o + 32));
    o += 32;
  }

  const recentBlockhash = bytes.slice(o, o + 32);
  o += 32;

  let numIx;
  [numIx, o] = readCompactU16(bytes, o);
  const instructions = [];
  for (let i = 0; i < numIx; i++) {
    const programIdIndex = bytes[o++];
    let nAcc;
    [nAcc, o] = readCompactU16(bytes, o);
    const accounts = Array.from(bytes.slice(o, o + nAcc));
    o += nAcc;
    let dataLen;
    [dataLen, o] = readCompactU16(bytes, o);
    const data = bytes.slice(o, o + dataLen);
    o += dataLen;
    instructions.push({ programIdIndex, accounts, data });
  }

  return {
    numRequiredSignatures,
    numReadonlySigned,
    numReadonlyUnsigned,
    accountKeys,
    recentBlockhash,
    instructions,
  };
}

// Solana's compact-u16 ("shortvec") length prefix: 7 bits per byte, LSB first,
// high bit signals "more bytes follow."
function readCompactU16(bytes, offset) {
  let value = 0;
  let shift = 0;
  let o = offset;
  for (;;) {
    const b = bytes[o++];
    value |= (b & 0x7f) << shift;
    if ((b & 0x80) === 0) break;
    shift += 7;
  }
  return [value, o];
}

function readU32LE(bytes, offset) {
  return (
    (bytes[offset] |
      (bytes[offset + 1] << 8) |
      (bytes[offset + 2] << 16) |
      (bytes[offset + 3] << 24)) >>>
    0
  );
}

function readU64LE(bytes, offset) {
  let value = 0n;
  for (let i = 7; i >= 0; i--) {
    value = (value << 8n) | BigInt(bytes[offset + i]);
  }
  return value;
}

function bytesEqual(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

function base64ToBytes(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function bytesToBase64(bytes) {
  let bin = "";
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
  return btoa(bin);
}
