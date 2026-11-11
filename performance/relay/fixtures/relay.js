// ../../../../../../private/tmp/relay-settlement/packages/lit-allocator/src/vm/ethereum-vm.ts
import { addr as addr2 } from "https://cdn.jsdelivr.net/npm/micro-eth-signer@0.18.1/+esm";
import { sign as secpSign } from "https://cdn.jsdelivr.net/npm/micro-eth-signer@0.18.1/utils.js/+esm";

// ../../../../../../private/tmp/relay-settlement/packages/lit-allocator/src/common/bytes.ts
function normalizeHex(hex, field = "hex") {
  const clean = String(hex ?? "").replace(/^0x/i, "");
  if (!/^[0-9a-fA-F]*$/.test(clean) || clean.length % 2 !== 0) {
    throw new Error(`invalid hex for ${field}: ${hex}`);
  }
  return clean.toLowerCase();
}
function hexToBytes(hex, field = "hex") {
  const clean = normalizeHex(hex, field);
  const bytes = new Uint8Array(clean.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}
function bytesToHex(bytes) {
  return Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("");
}
function concatBytes(...arrays) {
  const total = arrays.reduce((sum, arr) => sum + arr.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const arr of arrays) {
    out.set(arr, offset);
    offset += arr.length;
  }
  return out;
}
function expectLength(bytes, length, field) {
  if (bytes.length !== length) {
    throw new Error(`${field} must be exactly ${length} bytes`);
  }
  return bytes;
}

// ../../../../../../private/tmp/relay-settlement/packages/lit-allocator/src/common/crypto.ts
import { keccak_256 } from "https://cdn.jsdelivr.net/npm/@noble/hashes@1.8.0/sha3/+esm";
var DOMAIN_SEPARATOR = "relay-allocator";
var encoder = new TextEncoder();
function keccak(bytes) {
  return new Uint8Array(keccak_256(bytes));
}
async function hmacSha256(key, data) {
  const cryptoKey = await crypto.subtle.importKey(
    "raw",
    key,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"]
  );
  const mac = await crypto.subtle.sign("HMAC", cryptoKey, data);
  return new Uint8Array(mac);
}
async function hkdfExtract(salt, ikm) {
  return hmacSha256(salt, ikm);
}
async function hkdfExpand(prk, info, length) {
  const hashLen = 32;
  const n = Math.ceil(length / hashLen);
  const okm = new Uint8Array(n * hashLen);
  let prev = new Uint8Array(0);
  for (let i = 1; i <= n; i++) {
    const input = new Uint8Array(prev.length + info.length + 1);
    input.set(prev, 0);
    input.set(info, prev.length);
    input[prev.length + info.length] = i;
    prev = await hmacSha256(prk, input);
    okm.set(prev, (i - 1) * hashLen);
  }
  return new Uint8Array(okm.slice(0, length));
}
async function deriveKey(pkpPrivateKeyHex, vmTypeInfo) {
  const ikm = hexToBytes(pkpPrivateKeyHex, "pkpPrivateKey");
  const salt = encoder.encode(DOMAIN_SEPARATOR);
  const info = encoder.encode(vmTypeInfo);
  const prk = await hkdfExtract(salt, ikm);
  return hkdfExpand(prk, info, 32);
}

// ../../../../../../private/tmp/relay-settlement/packages/lit-allocator/src/common/attestation.ts
import { addr, verifyTyped } from "https://cdn.jsdelivr.net/npm/micro-eth-signer@0.18.1/+esm";

// ../../../../../../private/tmp/relay-settlement/packages/lit-allocator/src/common/abi.ts
function parseQuantity(value, field) {
  const s = String(value ?? "");
  if (s.length === 0) {
    throw new Error(`invalid quantity for ${field}`);
  }
  return BigInt(s);
}
function quantityToWord(value, field) {
  const n = parseQuantity(value, field);
  if (n < 0n || n >= 1n << 256n) {
    throw new Error(`${field} does not fit uint256`);
  }
  const out = new Uint8Array(32);
  let x = n;
  for (let i = 31; i >= 0; i--) {
    out[i] = Number(x & 0xffn);
    x >>= 8n;
  }
  return out;
}
function padRight32(bytes) {
  const len = Math.ceil(bytes.length / 32) * 32;
  const out = new Uint8Array(len);
  out.set(bytes);
  return out;
}
function abiEncodeString(value) {
  return abiEncodeBytes(new TextEncoder().encode(value));
}
function abiEncodeBytes(bytes) {
  return concatBytes(quantityToWord(bytes.length, "bytes.length"), padRight32(bytes));
}
function abiEncodeWithdrawRequest(req) {
  const dynamicParts = [
    abiEncodeString(req.chainId),
    abiEncodeBytes(hexToBytes(req.depository, "depository")),
    abiEncodeBytes(hexToBytes(req.currency, "currency")),
    null,
    // index 3 is the inline `amount`
    abiEncodeString(req.spenderChainId),
    abiEncodeBytes(hexToBytes(req.spender, "spender")),
    abiEncodeBytes(hexToBytes(req.receiver, "receiver")),
    abiEncodeBytes(hexToBytes(req.data, "data")),
    null
    // index 8 is the inline `nonce`
  ];
  const nonce = hexToBytes(req.nonce, "nonce");
  if (nonce.length !== 32) {
    throw new Error("withdraw request nonce must be exactly 32 bytes");
  }
  const head = [];
  const tail = [];
  let offset = 32 * 9;
  for (let i = 0; i < 9; i++) {
    if (i === 3) {
      head.push(quantityToWord(req.amount, "amount"));
    } else if (i === 8) {
      head.push(nonce);
    } else {
      const part = dynamicParts[i];
      if (!part) {
        throw new Error(`missing ABI dynamic part at index ${i}`);
      }
      head.push(quantityToWord(offset, "offset"));
      tail.push(part);
      offset += part.length;
    }
  }
  const tupleBody = concatBytes(...head, ...tail);
  return concatBytes(quantityToWord(32, "tupleOffset"), tupleBody);
}
function computeWithdrawRequestHash(req) {
  return keccak(abiEncodeWithdrawRequest(req));
}

// ../../../../../../private/tmp/relay-settlement/packages/lit-allocator/src/common/attestation.ts
var ATTESTATION_EIP712_TYPES = {
  WithdrawRequest: [
    { name: "chainId", type: "uint256" },
    { name: "allocator", type: "address" },
    { name: "withdrawRequestHash", type: "bytes32" },
    { name: "hashesToSign", type: "bytes32[]" }
  ]
};
function buildAttestationTypedData(attestation, withdrawRequestHashBytes, hashesToSignBytes) {
  return {
    types: ATTESTATION_EIP712_TYPES,
    primaryType: "WithdrawRequest",
    domain: {
      name: "WithdrawRequest",
      version: "1",
      chainId: attestation.chainId,
      verifyingContract: "0x0000000000000000000000000000000000000000"
    },
    message: {
      chainId: BigInt(attestation.chainId),
      allocator: attestation.allocator,
      withdrawRequestHash: withdrawRequestHashBytes,
      hashesToSign: hashesToSignBytes
    }
  };
}
function normalizeAddress(address, field) {
  return addr.addChecksum(`0x${bytesToHex(expectLength(hexToBytes(address, field), 20, field))}`);
}
function verifyWithdrawRequestAttestation(withdrawRequest, attestation, allocatorAddress, hubEvmChainId, allowedOracles, oracleSignatureThreshold) {
  if (attestation.chainId !== hubEvmChainId) {
    throw new Error(
      `attestation chainId mismatch: expected=${hubEvmChainId}, provided=${attestation.chainId}`
    );
  }
  const expectedAllocator = normalizeAddress(allocatorAddress, "allocatorAddress");
  const providedAllocator = normalizeAddress(attestation.allocator, "attestation.allocator");
  if (expectedAllocator !== providedAllocator) {
    throw new Error(
      `attestation allocator mismatch: expected=${expectedAllocator}, provided=${providedAllocator}`
    );
  }
  const expectedHash = computeWithdrawRequestHash(withdrawRequest);
  const expectedHashHex = `0x${bytesToHex(expectedHash)}`;
  if (attestation.withdrawRequestHash.toLowerCase() !== expectedHashHex.toLowerCase()) {
    throw new Error(
      `attestation withdrawRequestHash mismatch: expected=${expectedHashHex}, provided=${attestation.withdrawRequestHash}`
    );
  }
  if (!Array.isArray(attestation.hashesToSign) || attestation.hashesToSign.length === 0) {
    throw new Error("attestation hashesToSign must be a non-empty array");
  }
  const hashesToSignBytes = attestation.hashesToSign.map(
    (h, i) => expectLength(
      hexToBytes(h, `attestation.hashesToSign[${i}]`),
      32,
      `attestation.hashesToSign[${i}]`
    )
  );
  const typedData = buildAttestationTypedData(attestation, expectedHash, hashesToSignBytes);
  const allowed = new Set(
    allowedOracles.map((o, i) => normalizeAddress(o, `allowedOracles[${i}]`))
  );
  const verified = /* @__PURE__ */ new Set();
  for (const sig of attestation.signatures) {
    const oracle = normalizeAddress(sig.oracleSigner, "attestation.signatures[].oracleSigner");
    if (!allowed.has(oracle)) {
      throw new Error(`attestation oracle is not allowlisted: ${sig.oracleSigner}`);
    }
    if (!verifyTyped(sig.signature, typedData, oracle)) {
      throw new Error(`attestation signature mismatch for oracle ${sig.oracleSigner}`);
    }
    verified.add(oracle);
  }
  if (verified.size < oracleSignatureThreshold) {
    throw new Error(
      `attestation signature threshold not met: required=${oracleSignatureThreshold}, verified=${verified.size}`
    );
  }
  return { withdrawRequestHash: expectedHash, hashesToSign: hashesToSignBytes };
}

// ../../../../../../private/tmp/relay-settlement/packages/lit-allocator/src/config.ts
var ALLOCATOR_ADDRESS = "0x1563915e194D8CfBA1943570603F7606A3115508";
var HUB_EVM_CHAIN_ID = Number.parseInt("8453", 10);
var ALLOWED_ORACLES = JSON.parse('["0x19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A"]');
var ORACLE_SIGNATURE_THRESHOLD = Number.parseInt("1", 10);

// ../../../../../../private/tmp/relay-settlement/packages/lit-allocator/src/vm/ethereum-vm.ts
async function deriveEthereumPrivateKeyHex(pkpPrivateKeyHex) {
  return bytesToHex(await deriveKey(pkpPrivateKeyHex, "ethereum-vm"));
}
function ethereumAddressFromPrivateKey(privateKeyHex) {
  return addr2.fromPrivateKey(`0x${privateKeyHex}`);
}
function signDigest(messageHash, privateKeyHex) {
  const recovered = secpSign(messageHash, hexToBytes(`0x${privateKeyHex}`)).toBytes("recovered");
  const signature = new Uint8Array(65);
  signature.set(recovered.slice(1), 0);
  signature[64] = recovered[0] === 0 ? 27 : 28;
  return `0x${bytesToHex(signature)}`;
}
async function main({
  pkpId,
  action,
  withdrawRequest,
  attestation
}) {
  const pkpPrivateKey = await Lit.Actions.getPrivateKey({ pkpId });
  const ethereumPrivateKey = await deriveEthereumPrivateKeyHex(pkpPrivateKey);
  if (action === "wallet") {
    return {
      vmType: "ethereum-vm",
      address: ethereumAddressFromPrivateKey(ethereumPrivateKey)
    };
  }
  if (action === "sign") {
    if (!withdrawRequest) {
      throw new Error("withdrawRequest is required");
    }
    if (!attestation) {
      throw new Error("attestation is required");
    }
    const { withdrawRequestHash, hashesToSign } = verifyWithdrawRequestAttestation(
      withdrawRequest,
      attestation,
      ALLOCATOR_ADDRESS,
      HUB_EVM_CHAIN_ID,
      ALLOWED_ORACLES,
      ORACLE_SIGNATURE_THRESHOLD
    );
    return {
      vmType: "ethereum-vm",
      address: ethereumAddressFromPrivateKey(ethereumPrivateKey),
      withdrawRequestHash: `0x${bytesToHex(withdrawRequestHash)}`,
      results: hashesToSign.map((hash) => ({
        hash: `0x${bytesToHex(hash)}`,
        signature: signDigest(hash, ethereumPrivateKey)
      }))
    };
  }
  throw new Error(`unknown action: ${action}`);
}
