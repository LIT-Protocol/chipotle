// Lit Action: a keyless Zcash (transparent) wallet bound to this exact code.
//
// The wallet's secp256k1 keypair is the action's own identity key
// (Lit.Actions.getLitActionPrivateKey(), itself derived from the action's
// IPFS CID). Zcash transparent ("t1…") addresses are secp256k1/P2PKH — the
// SAME curve Lit already gives you — so unlike the Solana example there is no
// ed25519 bridge: the identity key IS the Zcash key. What's Zcash-specific is
// the ADDRESS ENCODING (Base58Check with a two-byte version prefix) and, above
// all, the SIGNATURE HASH: Zcash signs transparent inputs with a BLAKE2b-256
// digest personalized by the consensus branch ID (ZIP-243), not Bitcoin's
// double-SHA256 and not an EVM keccak hash. That sighash is what this example
// teaches.
//
// The key is derived from the CID and never leaves the Lit TEE, so the wallet
// is bound to the code: edit a byte and the CID changes, the identity key
// changes, and the t1 address changes. This exact action is the only thing
// that can ever sign for that address — no key file to steal, no PKP to mint.
//
// Two operations, selected by `action`:
//   action: "address"  -> return the action's Zcash t1 address (Base58Check)
//   action: "sign"     -> validate a requested transparent spend against
//                         policy, build the v4 transaction itself, ZIP-243
//                         sign every input, and return the raw tx hex
//
// The signer does NOT trust the client to tell it what the transaction is. The
// client supplies UTXOs (txid/vout/value) and a recipient+amount; the action
// builds every output itself — the recipient output (capped) plus a change
// output that can ONLY go back to its own address — computes the sighash for
// each input from those bytes, signs, and emits the exact hex to broadcast. A
// caller cannot redirect the change, exceed the spend cap, or burn an
// arbitrary fee. (Lying about an input's value or outpoint can't steal funds
// either: the value is committed in the ZIP-243 sighash and the outpoint must
// reference a real UTXO, so a lie just makes the broadcast fail.)
//
// js_params:
//   action        "address" | "sign"
//   inputs        (sign) [{ txid: hex(big-endian, as shown in explorers),
//                           vout: number, value: string(zatoshi) }]
//   recipient     (sign) destination t1 address (Base58Check)
//   amountZat     (sign) string, zatoshi to send to the recipient
//   feeZat        (sign) string, zatoshi miner fee
//   expiryHeight  (sign) number, nExpiryHeight (block height the tx expires at)
//
// Imports are pinned ESM from jsDelivr (see docs/lit-actions/imports.mdx).
// @noble/* and @scure/* are pure ESM with no Node built-ins, so they run as-is
// inside the action runtime.
import * as secp from "@noble/secp256k1@2.1.0";
import { blake2b } from "@noble/hashes@1.4.0/blake2b/+esm";
import { sha256 } from "@noble/hashes@1.4.0/sha256/+esm";
import { ripemd160 } from "@noble/hashes@1.4.0/ripemd160/+esm";
import { hmac } from "@noble/hashes@1.4.0/hmac/+esm";
import { base58 } from "@scure/base@1.1.6";

// @noble/secp256k1 v2 needs an hmac-sha256 wired in for its sync signing API
// (RFC 6979 deterministic nonces).
secp.etc.hmacSha256Sync = (key, ...msgs) =>
  hmac(sha256, key, secp.etc.concatBytes(...msgs));

const { hexToBytes, bytesToHex, concatBytes } = secp.etc;

// ---------------------------------------------------------------------------
// Policy + network constants. All of this is part of the hashed source, so
// changing any of it changes the CID and therefore the wallet address — the
// caps are bound to the address exactly like the key is.
// ---------------------------------------------------------------------------

// Mainnet P2PKH ("t1…") Base58Check version prefix. (Testnet "tm…" would be
// 0x1D,0x25 — but Zcash testnet REST infrastructure is effectively dead, so
// this example targets mainnet. See the README.)
const P2PKH_PREFIX = new Uint8Array([0x1c, 0xb8]);

// Consensus branch ID for the active network upgrade, little-endian. This is
// NU6 (0xc8e71055). It is the same on mainnet and testnet and only changes at
// a network upgrade — bump it (and re-derive: the CID won't change, only the
// signatures) when the next upgrade activates.
const BRANCH_ID_LE = new Uint8Array([0x55, 0x10, 0xe7, 0xc8]);

// The most this wallet will ever pay a recipient in one transaction: 0.01 ZEC.
const MAX_AMOUNT_ZAT = 1_000_000n;
// The most it will ever burn as a miner fee: 0.0005 ZEC. Caps the only value
// a caller could otherwise leak by over-funding the fee.
const MAX_FEE_ZAT = 50_000n;
// Below this, a change output isn't worth creating; fold it into the fee.
const DUST_ZAT = 1_000n;

// v4 (Sapling) transaction header: fOverwintered bit (31) set | version 4.
const HEADER = u32le(0x80000004);
// SAPLING_VERSION_GROUP_ID.
const VERSION_GROUP_ID = u32le(0x892f2085);
// SIGHASH_ALL.
const SIGHASH_ALL = 0x01;
const N_SEQUENCE = 0xffffffff;
const SEQUENCE_BYTES = u32le(N_SEQUENCE);
const ZERO32 = new Uint8Array(32);

async function main({ action, inputs, recipient, amountZat, feeZat, expiryHeight }) {
  // The identity key is a 32-byte secp256k1 private key — already exactly a
  // Zcash transparent private key. No curve bridge needed.
  const priv = hexToBytes(
    (await Lit.Actions.getLitActionPrivateKey()).replace(/^0x/, "")
  );
  const pubkey = secp.getPublicKey(priv, true); // 33-byte compressed
  const selfHash160 = hash160(pubkey);
  const address = base58CheckEncode(concatBytes(P2PKH_PREFIX, selfHash160));

  if (action === "address") {
    return { address };
  }
  if (action !== "sign") {
    return { authorized: false, reason: `unknown action "${action}"` };
  }

  // ---- Validate the requested spend against policy ------------------------
  if (!Array.isArray(inputs) || inputs.length === 0) {
    return { authorized: false, reason: "no inputs provided" };
  }
  if (!Number.isInteger(expiryHeight) || expiryHeight <= 0) {
    return { authorized: false, reason: "expiryHeight must be a positive integer" };
  }

  let amount, fee, totalIn;
  try {
    amount = BigInt(amountZat);
    fee = BigInt(feeZat);
    totalIn = inputs.reduce((sum, i) => sum + BigInt(i.value), 0n);
  } catch {
    return { authorized: false, reason: "amount/fee/value must be integer zatoshi strings" };
  }

  if (amount <= 0n) {
    return { authorized: false, reason: "amount must be positive" };
  }
  if (amount > MAX_AMOUNT_ZAT) {
    return {
      authorized: false,
      reason: `amount ${amount} zat exceeds cap ${MAX_AMOUNT_ZAT}`,
    };
  }
  if (fee < 0n || fee > MAX_FEE_ZAT) {
    return {
      authorized: false,
      reason: `fee ${fee} zat outside [0, ${MAX_FEE_ZAT}]`,
    };
  }

  // The recipient must be a mainnet t1 P2PKH address. Decode it ourselves so a
  // typo or wrong-network address is refused rather than signed.
  let recipientHash160;
  try {
    const payload = base58CheckDecode(recipient);
    if (payload[0] !== P2PKH_PREFIX[0] || payload[1] !== P2PKH_PREFIX[1]) {
      return { authorized: false, reason: "recipient is not a mainnet t1 P2PKH address" };
    }
    recipientHash160 = payload.slice(2);
    if (recipientHash160.length !== 20) {
      return { authorized: false, reason: "recipient hash160 is not 20 bytes" };
    }
  } catch (e) {
    return { authorized: false, reason: `recipient is not valid Base58Check: ${e.message}` };
  }

  // change = totalIn - amount - fee, and it can ONLY ever go back to us.
  let change = totalIn - amount - fee;
  if (change < 0n) {
    return {
      authorized: false,
      reason: `inputs (${totalIn} zat) do not cover amount + fee (${amount + fee} zat)`,
    };
  }
  // Sub-dust change isn't worth a 34-byte output; fold it into the fee — but
  // only if that keeps the fee under the cap, so this can't become a leak.
  if (change > 0n && change < DUST_ZAT) {
    fee += change;
    change = 0n;
    if (fee > MAX_FEE_ZAT) {
      return { authorized: false, reason: "dust change would push the fee over the cap; adjust inputs" };
    }
  }

  // The action builds every output. There is no path for the caller to add an
  // output of its own.
  const outputs = [{ script: p2pkhScript(recipientHash160), value: amount }];
  if (change > 0n) {
    outputs.push({ script: p2pkhScript(selfHash160), value: change });
  }

  // ---- Build the canonical pieces shared by every input's sighash ---------
  const selfScript = p2pkhScript(selfHash160); // scriptCode — all inputs are ours

  const hashPrevouts = blake2b16(
    concatBytes(...inputs.map((i) => outpoint(i.txid, i.vout))),
    pers("ZcashPrevoutHash")
  );
  const hashSequence = blake2b16(
    concatBytes(...inputs.map(() => SEQUENCE_BYTES)),
    pers("ZcashSequencHash")
  );
  const hashOutputs = blake2b16(
    concatBytes(...outputs.map((o) => concatBytes(u64le(o.value), withLen(o.script)))),
    pers("ZcashOutputsHash")
  );

  const expiryBytes = u32le(expiryHeight);
  const preamble = concatBytes(
    HEADER,
    VERSION_GROUP_ID,
    hashPrevouts,
    hashSequence,
    hashOutputs,
    ZERO32, // hashJoinSplits     — none
    ZERO32, // hashShieldedSpends — none
    ZERO32, // hashShieldedOutputs — none
    u32le(0), // nLockTime
    expiryBytes, // nExpiryHeight
    u64le(0n), // valueBalance (Sapling net value) — 0
    u32le(SIGHASH_ALL) // nHashType
  );

  // ---- Sign each input over its ZIP-243 sighash and build its scriptSig ----
  const scriptSigs = inputs.map((i) => {
    const preimage = concatBytes(
      preamble,
      outpoint(i.txid, i.vout),
      withLen(selfScript), // scriptCode of the output being spent (ours)
      u64le(BigInt(i.value)), // amount of that output — committed in the sighash
      SEQUENCE_BYTES
    );
    const sighash = blake2b16(preimage, pers("ZcashSigHash", BRANCH_ID_LE));
    // ECDSA over the 32-byte digest directly. @noble/secp256k1 v2 signs low-S
    // (canonical) by default but only emits a 64-byte compact r||s — Zcash
    // scriptSigs need DER, so we encode it ourselves.
    const compact = secp.sign(sighash, priv).toCompactRawBytes();
    const sig = concatBytes(derEncode(compact), new Uint8Array([SIGHASH_ALL]));
    return concatBytes(pushData(sig), pushData(pubkey));
  });

  // ---- Assemble the final v4 transaction ----------------------------------
  const tx = concatBytes(
    HEADER,
    VERSION_GROUP_ID,
    compactSize(inputs.length),
    ...inputs.map((i, n) =>
      concatBytes(outpoint(i.txid, i.vout), withLen(scriptSigs[n]), SEQUENCE_BYTES)
    ),
    compactSize(outputs.length),
    ...outputs.map((o) => concatBytes(u64le(o.value), withLen(o.script))),
    u32le(0), // nLockTime
    expiryBytes, // nExpiryHeight
    u64le(0n), // valueBalance
    compactSize(0), // nShieldedSpend
    compactSize(0), // nShieldedOutput
    compactSize(0) // nJoinSplit
  );

  // v4 txid is the double-SHA256 of the whole serialization, shown reversed.
  const txid = bytesToHex(sha256(sha256(tx)).reverse());

  return {
    authorized: true,
    address,
    recipient,
    amountZat: amount.toString(),
    feeZat: fee.toString(),
    changeZat: change.toString(),
    txid,
    txHex: bytesToHex(tx),
  };
}

// ---------------------------------------------------------------------------
// Zcash / Bitcoin-style serialization helpers.
// ---------------------------------------------------------------------------

// An outpoint is the spent txid in INTERNAL (little-endian) byte order — the
// reverse of how explorers display it — followed by the 4-byte output index.
function outpoint(txidHexBigEndian, vout) {
  return concatBytes(hexToBytes(txidHexBigEndian).reverse(), u32le(vout));
}

// scriptPubKey / scriptCode for a P2PKH output:
//   OP_DUP OP_HASH160 <20-byte hash160> OP_EQUALVERIFY OP_CHECKSIG
function p2pkhScript(hash160Bytes) {
  return concatBytes(
    new Uint8Array([0x76, 0xa9, 0x14]),
    hash160Bytes,
    new Uint8Array([0x88, 0xac])
  );
}

function hash160(bytes) {
  return ripemd160(sha256(bytes));
}

// DER-encode a 64-byte compact (r || s) ECDSA signature:
//   0x30 <len> 0x02 <rlen> <r> 0x02 <slen> <s>
// r and s are minimal big-endian: leading zero bytes stripped, with a single
// 0x00 prepended if the high bit is set (so they stay positive). s is already
// low-S because @noble signs canonically — we only re-encode it.
function derEncode(compact) {
  const r = derInt(compact.slice(0, 32));
  const s = derInt(compact.slice(32, 64));
  const seqLen = 2 + r.length + 2 + s.length;
  return concatBytes(
    new Uint8Array([0x30, seqLen, 0x02, r.length]),
    r,
    new Uint8Array([0x02, s.length]),
    s
  );
}

function derInt(bytes) {
  let i = 0;
  while (i < bytes.length - 1 && bytes[i] === 0) i++;
  let v = bytes.slice(i);
  if (v[0] & 0x80) v = concatBytes(new Uint8Array([0x00]), v);
  return v;
}

// BLAKE2b-256 with a 16-byte personalization (Zcash's domain separation).
function blake2b16(data, personalization) {
  return blake2b(data, { dkLen: 32, personalization });
}

// Build a 16-byte BLAKE2b personalization from an ASCII tag (+ optional suffix
// bytes, e.g. the consensus branch ID for the signature hash).
function pers(tag, suffix) {
  const base = new Uint8Array(tag.length);
  for (let i = 0; i < tag.length; i++) base[i] = tag.charCodeAt(i);
  return suffix ? concatBytes(base, suffix) : base;
}

// Prefix a byte string with its CompactSize ("varint") length.
function withLen(bytes) {
  return concatBytes(compactSize(bytes.length), bytes);
}

// Bitcoin script push of <= 520 bytes of data (we only ever push <= 256).
function pushData(bytes) {
  if (bytes.length <= 75) {
    return concatBytes(new Uint8Array([bytes.length]), bytes);
  }
  if (bytes.length <= 255) {
    return concatBytes(new Uint8Array([0x4c, bytes.length]), bytes);
  }
  throw new Error("pushData operand too large for this example");
}

function compactSize(n) {
  if (n < 0xfd) return new Uint8Array([n]);
  if (n <= 0xffff) return new Uint8Array([0xfd, n & 0xff, (n >> 8) & 0xff]);
  if (n <= 0xffffffff) return concatBytes(new Uint8Array([0xfe]), u32le(n));
  return concatBytes(new Uint8Array([0xff]), u64le(BigInt(n)));
}

function u32le(n) {
  const b = new Uint8Array(4);
  b[0] = n & 0xff;
  b[1] = (n >>> 8) & 0xff;
  b[2] = (n >>> 16) & 0xff;
  b[3] = (n >>> 24) & 0xff;
  return b;
}

function u64le(big) {
  const b = new Uint8Array(8);
  let v = BigInt(big);
  for (let i = 0; i < 8; i++) {
    b[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return b;
}

// ---------------------------------------------------------------------------
// Base58Check (version-prefixed payload + 4-byte double-SHA256 checksum).
// ---------------------------------------------------------------------------
function base58CheckEncode(payload) {
  const checksum = sha256(sha256(payload)).slice(0, 4);
  return base58.encode(concatBytes(payload, checksum));
}

function base58CheckDecode(str) {
  const raw = base58.decode(str);
  if (raw.length < 5) throw new Error("too short");
  const payload = raw.slice(0, raw.length - 4);
  const checksum = raw.slice(raw.length - 4);
  const expected = sha256(sha256(payload)).slice(0, 4);
  for (let i = 0; i < 4; i++) {
    if (checksum[i] !== expected[i]) throw new Error("checksum mismatch");
  }
  return payload;
}
