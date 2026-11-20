/**
 * Lit API attestation verifier.
 *
 * Proves, from the client, that a Lit API origin is the governed Lit workload
 * running inside a genuine confidential VM, before any key or secret is sent
 * to it. Dependency-free: WebCrypto and fetch only (Node.js 22+ and browsers).
 *
 * What `verifyAttestation` establishes for an AMD SEV-SNP deployment:
 *
 *  1. Hardware. The attestation report is signed by a chip key (VCEK) that
 *     chains through AMD's intermediate (ASK) to an AMD root key (ARK) pinned
 *     in this file. The VCEK is bound to the reporting chip and its firmware.
 *  2. Freshness. The signed report carries a commitment to a nonce this client
 *     just generated, so an old report cannot be replayed.
 *  3. Launch state. The signed launch measurement is recomputed from the
 *     firmware, kernel, initrd and kernel command line the VM declares, which
 *     ties the report to one dstack OS image. Debug and migration are refused.
 *  4. Workload. The signed HOST_DATA binds the app id and the hash of the
 *     docker-compose document the VM runs.
 *  5. Governance. The app id is the expected Lit DstackApp contract, and that
 *     contract and its KMS contract (on Base) allowlist this compose hash and
 *     OS image. Only a multisig-approved release passes.
 *  6. Channel (Node.js only, when the deployment publishes it). The TLS
 *     certificate of this very connection was issued to a key held inside the
 *     same attested app.
 *
 * Intel TDX evidence is recognised and rejected with `unsupported_platform`:
 * use the legacy TDX origin and verifier for it. Unknown platforms and
 * envelope versions fail closed.
 */

// ---------------------------------------------------------------------------
// Origins and policies
// ---------------------------------------------------------------------------

/** Public Base mainnet RPC endpoints, tried in order. Pin your own for stronger guarantees. */
export const BASE_PUBLIC_RPC_URLS = Object.freeze([
  'https://mainnet.base.org',
  'https://base-rpc.publicnode.com',
  'https://1rpc.io/base',
  'https://base.drpc.org',
]);

/** dstack KMS contract on Base that allowlists OS images for every Lit app. */
const LIT_KMS_CONTRACT = '0x2f83172a49584c017f2b256f0fb2dca14126ba9c';

/**
 * @typedef {Object} AttestationPolicy
 * @property {string} appId - Expected dstack app id (the DstackApp contract address), 40 hex chars.
 * @property {string} kmsContract - DstackKms contract address that allowlists OS images.
 * @property {string[]} [rpcUrls] - JSON-RPC endpoints for the chain hosting both contracts.
 * @property {string[]} [platforms] - Accepted platforms. Default `['sev-snp']`.
 * @property {boolean} [requireUpToDateTcb] - Reject a platform whose reported, current, committed and launch TCB differ.
 * @property {{bootloader?: number, tee?: number, snp?: number, microcode?: number, fmc?: number}} [minTcb] - Minimum reported firmware versions.
 * @property {'auto'|'require'|'strict'|'off'} [tlsBinding] - TLS channel binding. `'auto'` (default for a
 *   policy that does not say): checked when the runtime and the deployment support it. `'require'`: in a runtime
 *   that can inspect TLS certificates (Node.js) the binding must verify; a browser, which cannot, proceeds and
 *   reports `'unsupported'`. `'strict'`: must verify everywhere, so it fails in browsers.
 */

/**
 * Production policy. Firmware currency is not checked here by default: the
 * app's on-chain `requireTcbUpToDate` makes the KMS refuse keys to a box that
 * is not up to date when it boots. `requireUpToDateTcb` adds a check on every
 * verification, at the price of failing closed while the platform's firmware
 * is updated under a running VM.
 */
export const LIT_PRODUCTION_POLICY = Object.freeze({
  appId: '3f91deaf16ff7c823ee65081d6bafa1ceea05ffc',
  kmsContract: LIT_KMS_CONTRACT,
  rpcUrls: BASE_PUBLIC_RPC_URLS,
  tlsBinding: 'require',
});

/** Staging ("next") policy. */
export const LIT_STAGING_POLICY = Object.freeze({
  appId: '969a8c14c9e13420705b19c7246aeed27897e7ea',
  kmsContract: LIT_KMS_CONTRACT,
  rpcUrls: BASE_PUBLIC_RPC_URLS,
  tlsBinding: 'require',
});

/** Production API origin served by the platform-attested deployment. */
export const LIT_API_URL = 'https://api2.chipotle.litprotocol.com';
/** Staging API origin served by the platform-attested deployment. */
export const LIT_STAGING_API_URL = 'https://test2.chipotle.litprotocol.com';

/**
 * Origins this SDK verifies by default. A client pointed at one of these runs
 * `verifyAttestation` before its first request unless attestation is disabled.
 */
export const ATTESTED_ORIGINS = Object.freeze({
  [LIT_API_URL]: LIT_PRODUCTION_POLICY,
  [LIT_STAGING_API_URL]: LIT_STAGING_POLICY,
});

/**
 * Origins being retired, mapped to their successor. They keep working until
 * their announced sunset date; this SDK does not verify them.
 */
export const LEGACY_ORIGINS = Object.freeze({
  'https://api.chipotle.litprotocol.com': LIT_API_URL,
  'https://test.chipotle.litprotocol.com': LIT_STAGING_API_URL,
});

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/**
 * Thrown for every verification failure. `code` is stable; `message` is for
 * people. Codes:
 *  - `invalid_argument`       bad caller input
 *  - `no_policy`              origin has no known policy and none was given
 *  - `endpoint_unavailable`   the attestation endpoint could not be reached
 *  - `endpoint_busy`          the endpoint was still busy after every retry
 *  - `malformed_evidence`     evidence could not be decoded
 *  - `unsupported_platform`   evidence is for a platform this verifier or policy does not accept
 *  - `collateral_unavailable` AMD endorsement certificates could not be obtained
 *  - `untrusted_hardware`     certificate chain or report signature is invalid
 *  - `stale_evidence`         the signed report does not commit to this request's nonce
 *  - `policy_violation`       the guest launch policy or TCB is not acceptable
 *  - `measurement_mismatch`   declared launch inputs do not reproduce the signed measurement
 *  - `identity_mismatch`      app identity is not bound to the report or is not the expected app
 *  - `not_governed`           compose hash or OS image is not allowlisted on-chain
 *  - `governance_unavailable` no RPC endpoint answered the on-chain check
 *  - `tls_binding_failed`     the TLS certificate is not bound to the attested app
 *  - `tls_binding_unavailable` TLS binding was required but the deployment or runtime cannot provide it
 */
export class AttestationError extends Error {
  constructor(code, message, options) {
    super(`Attestation: ${message}`, options);
    this.name = 'AttestationError';
    this.code = code;
  }
}
const fail = (code, message, cause) => {
  throw new AttestationError(code, message, cause === undefined ? undefined : { cause });
};

// ---------------------------------------------------------------------------
// Bytes
// ---------------------------------------------------------------------------

const subtle = () => {
  const s = globalThis.crypto?.subtle;
  if (!s) fail('invalid_argument', 'WebCrypto (crypto.subtle) is required');
  return s;
};
const utf8 = (s) => new TextEncoder().encode(s);
const text = (b) => new TextDecoder('utf-8', { fatal: true }).decode(b);
const toHex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
function fromHex(hex, what = 'hex value') {
  if (typeof hex !== 'string' || hex.length % 2 !== 0 || !/^[0-9a-fA-F]*$/.test(hex)) {
    fail('malformed_evidence', `${what} is not valid hex`);
  }
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.substr(i * 2, 2), 16);
  return out;
}
function fromBase64(b64, what = 'base64 value') {
  if (typeof b64 !== 'string') fail('malformed_evidence', `${what} is not base64`);
  let bin;
  try {
    bin = atob(b64);
  } catch (e) {
    fail('malformed_evidence', `${what} is not base64`, e);
  }
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}
function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}
const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
const isZero = (b) => b.every((v) => v === 0);
const sha256 = async (b) => new Uint8Array(await subtle().digest('SHA-256', b));
const sha384 = async (b) => new Uint8Array(await subtle().digest('SHA-384', b));
const strip0x = (s) => String(s).trim().toLowerCase().replace(/^0x/, '');
function hexOfLength(value, bytes, what, code = 'invalid_argument') {
  const hex = strip0x(value);
  if (!new RegExp(`^[0-9a-f]{${bytes * 2}}$`).test(hex)) fail(code, `${what} must be ${bytes} bytes of hex`);
  return hex;
}

// ---------------------------------------------------------------------------
// Keccak-256 (the challenge commitment; WebCrypto has no Keccak)
// ---------------------------------------------------------------------------

const KECCAK_RC = [
  0x0000000000000001n, 0x0000000000008082n, 0x800000000000808an, 0x8000000080008000n,
  0x000000000000808bn, 0x0000000080000001n, 0x8000000080008081n, 0x8000000000008009n,
  0x000000000000008an, 0x0000000000000088n, 0x0000000080008009n, 0x000000008000000an,
  0x000000008000808bn, 0x800000000000008bn, 0x8000000000008089n, 0x8000000000008003n,
  0x8000000000008002n, 0x8000000000000080n, 0x000000000000800an, 0x800000008000000an,
  0x8000000080008081n, 0x8000000000008080n, 0x0000000080000001n, 0x8000000080008008n,
];
const KECCAK_ROT = [0, 1, 62, 28, 27, 36, 44, 6, 55, 20, 3, 10, 43, 25, 39, 41, 45, 15, 21, 8, 18, 2, 61, 56, 14];
const MASK64 = (1n << 64n) - 1n;
const rotl64 = (x, n) => (n === 0 ? x : ((x << BigInt(n)) | (x >> BigInt(64 - n))) & MASK64);

function keccakF(s) {
  for (let round = 0; round < 24; round++) {
    const c = [0, 1, 2, 3, 4].map((x) => s[x] ^ s[x + 5] ^ s[x + 10] ^ s[x + 15] ^ s[x + 20]);
    for (let x = 0; x < 5; x++) {
      const d = c[(x + 4) % 5] ^ rotl64(c[(x + 1) % 5], 1);
      for (let y = 0; y < 25; y += 5) s[x + y] ^= d;
    }
    const b = new Array(25);
    for (let x = 0; x < 5; x++) {
      for (let y = 0; y < 5; y++) {
        b[y + ((2 * x + 3 * y) % 5) * 5] = rotl64(s[x + 5 * y], KECCAK_ROT[x + 5 * y]);
      }
    }
    for (let y = 0; y < 25; y += 5) {
      for (let x = 0; x < 5; x++) s[x + y] = b[x + y] ^ (~b[((x + 1) % 5) + y] & MASK64 & b[((x + 2) % 5) + y]);
    }
    s[0] ^= KECCAK_RC[round];
  }
}

/** Keccak-256 (Ethereum's hash, not SHA3-256). */
export function keccak256(data) {
  const rate = 136;
  const padded = new Uint8Array(Math.ceil((data.length + 1) / rate) * rate);
  padded.set(data);
  padded[data.length] ^= 0x01;
  padded[padded.length - 1] ^= 0x80;
  const state = new Array(25).fill(0n);
  const view = new DataView(padded.buffer);
  for (let off = 0; off < padded.length; off += rate) {
    for (let i = 0; i < rate / 8; i++) state[i] ^= view.getBigUint64(off + i * 8, true);
    keccakF(state);
  }
  const out = new Uint8Array(32);
  const outView = new DataView(out.buffer);
  for (let i = 0; i < 4; i++) outView.setBigUint64(i * 8, state[i], true);
  return out;
}

const V3_DOMAIN = utf8('chipotle-attestation-v3:');

/** The 64 report-data bytes a v3 attestation for `nonce` (32 bytes) must carry. */
export function attestationV3ReportData(nonce) {
  if (!(nonce instanceof Uint8Array) || nonce.length !== 32) fail('invalid_argument', 'nonce must be 32 bytes');
  const out = new Uint8Array(64);
  out.set(keccak256(concat(V3_DOMAIN, nonce)));
  return out;
}

// ---------------------------------------------------------------------------
// Decoders: SCALE (dstack envelope v0), MessagePack (envelope v1), CBOR, DER
// ---------------------------------------------------------------------------

class Reader {
  constructor(bytes, what) {
    this.b = bytes;
    this.p = 0;
    this.what = what;
  }
  need(n) {
    if (!Number.isSafeInteger(n) || n < 0 || this.p + n > this.b.length) {
      fail('malformed_evidence', `${this.what} is truncated`);
    }
  }
  u8() {
    this.need(1);
    return this.b[this.p++];
  }
  take(n) {
    this.need(n);
    const out = this.b.subarray(this.p, this.p + n);
    this.p += n;
    return out;
  }
  uint(n) {
    let v = 0n;
    for (const byte of this.take(n)) v = (v << 8n) | BigInt(byte);
    return v <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(v) : v;
  }
  done() {
    if (this.p !== this.b.length) fail('malformed_evidence', `${this.what} has trailing bytes`);
  }
}

function scaleCompact(r) {
  const first = r.u8();
  const mode = first & 3;
  if (mode === 0) return first >> 2;
  if (mode === 1) return ((first | (r.u8() << 8)) >>> 0) >> 2;
  if (mode === 2) {
    const rest = r.take(3);
    return ((first | (rest[0] << 8) | (rest[1] << 16) | (rest[2] << 24)) >>> 0) >>> 2;
  }
  fail('malformed_evidence', 'attestation length field is too large');
}
const scaleBytes = (r) => r.take(scaleCompact(r));

/** Size in bytes above which an attestation envelope is refused before parsing. */
const MAX_EVIDENCE_BYTES = 2 * 1024 * 1024;
const SCALE_VARIANTS = ['tdx', 'gcp-tdx', 'nitro-enclave', 'sev-snp', 'aws-nitro-tpm'];

function decodeScaleEnvelope(bytes) {
  const r = new Reader(bytes, 'attestation');
  if (r.u8() !== 0) fail('malformed_evidence', 'unknown attestation envelope');
  const platform = SCALE_VARIANTS[r.u8()];
  if (!platform) fail('unsupported_platform', 'unknown platform in attestation');
  if (platform !== 'sev-snp') return { platform };
  const report = scaleBytes(r);
  const certChain = [];
  for (let n = scaleCompact(r); n > 0; n--) certChain.push(scaleBytes(r));
  const mrConfig = text(scaleBytes(r));
  // SEV-SNP has no runtime measurement register, so dstack emits no runtime
  // events for it. Anything else is a layout this decoder does not know.
  if (scaleCompact(r) !== 0) fail('malformed_evidence', 'unexpected runtime events in SEV-SNP attestation');
  r.take(64); // unsigned copy of report_data; the signed one is in the report
  const config = text(scaleBytes(r));
  r.done();
  return { platform, report, certChain, mrConfig, config };
}

function msgpack(r, depth = 0) {
  if (depth > 16) fail('malformed_evidence', 'attestation is nested too deeply');
  const t = r.u8();
  const map = (n) => {
    const out = {};
    for (; n > 0; n--) {
      const key = msgpack(r, depth + 1);
      if (typeof key !== 'string') fail('malformed_evidence', 'attestation map key is not a string');
      // A second value for a key would let two readers disagree about the document.
      if (Object.prototype.hasOwnProperty.call(out, key)) fail('malformed_evidence', `attestation repeats the key ${JSON.stringify(key)}`);
      out[key] = msgpack(r, depth + 1);
    }
    return out;
  };
  const arr = (n) => {
    const out = [];
    for (; n > 0; n--) out.push(msgpack(r, depth + 1));
    return out;
  };
  if (t <= 0x7f) return t;
  if (t <= 0x8f) return map(t & 0x0f);
  if (t <= 0x9f) return arr(t & 0x0f);
  if (t <= 0xbf) return text(r.take(t & 0x1f));
  if (t >= 0xe0) return t - 0x100;
  switch (t) {
    case 0xc0: return null;
    case 0xc2: return false;
    case 0xc3: return true;
    case 0xc4: return r.take(r.uint(1));
    case 0xc5: return r.take(r.uint(2));
    case 0xc6: return r.take(r.uint(4));
    case 0xcc: return r.uint(1);
    case 0xcd: return r.uint(2);
    case 0xce: return r.uint(4);
    case 0xcf: return r.uint(8);
    case 0xd9: return text(r.take(r.uint(1)));
    case 0xda: return text(r.take(r.uint(2)));
    case 0xdb: return text(r.take(r.uint(4)));
    case 0xdc: return arr(r.uint(2));
    case 0xdd: return arr(r.uint(4));
    case 0xde: return map(r.uint(2));
    case 0xdf: return map(r.uint(4));
    default: fail('malformed_evidence', `unsupported attestation encoding 0x${t.toString(16)}`);
  }
}

/** serde encodes a byte vector as either a binary string or an array of integers. */
function msgpackBytes(v, what) {
  if (v instanceof Uint8Array) return v;
  if (Array.isArray(v) && v.every((x) => Number.isInteger(x) && x >= 0 && x <= 255)) return Uint8Array.from(v);
  fail('malformed_evidence', `${what} is not a byte string`);
}

function decodeMsgpackEnvelope(bytes) {
  const r = new Reader(bytes, 'attestation');
  const doc = msgpack(r);
  r.done();
  if (doc?.version !== 1) fail('malformed_evidence', 'unsupported attestation envelope version');
  const platform = doc.platform?.kind;
  if (typeof platform !== 'string') fail('malformed_evidence', 'attestation has no platform');
  if (platform !== 'sev-snp') return { platform };
  const data = doc.platform.data ?? {};
  if (doc.stack?.kind !== 'dstack' && doc.stack?.kind !== 'dstack-pod') fail('malformed_evidence', 'unsupported attestation stack');
  const stack = doc.stack.data ?? {};
  if (!Array.isArray(data.cert_chain) || typeof data.mr_config !== 'string' || typeof stack.config !== 'string') {
    fail('malformed_evidence', 'SEV-SNP attestation is missing fields');
  }
  if (msgpackBytes(stack.report_data, 'stack report data').length !== 64) fail('malformed_evidence', 'stack report data must be 64 bytes');
  if (!Array.isArray(stack.runtime_events) || stack.runtime_events.length !== 0) {
    fail('malformed_evidence', 'unexpected runtime events in SEV-SNP attestation');
  }
  return {
    platform,
    report: msgpackBytes(data.report, 'report'),
    certChain: data.cert_chain.map((c) => msgpackBytes(c, 'certificate')),
    mrConfig: data.mr_config,
    config: stack.config,
  };
}

/**
 * Decodes a dstack VersionedAttestation. Nothing in the result is trusted
 * until `verifySnpEvidence` has checked it against the signed report.
 */
export function decodeAttestation(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length === 0) fail('malformed_evidence', 'attestation is empty');
  if (bytes.length > MAX_EVIDENCE_BYTES) fail('malformed_evidence', 'attestation is too large');
  const first = bytes[0];
  try {
    if (first === 0x00) return decodeScaleEnvelope(bytes);
    if ((first & 0xf0) === 0x80 || first === 0xde || first === 0xdf) return decodeMsgpackEnvelope(bytes);
  } catch (e) {
    if (e instanceof AttestationError) throw e;
    fail('malformed_evidence', 'attestation could not be decoded', e);
  }
  fail('malformed_evidence', 'unknown attestation envelope');
}

function cbor(r, depth = 0) {
  if (depth > 8) fail('malformed_evidence', 'measurement document is nested too deeply');
  const head = r.u8();
  const major = head >> 5;
  const info = head & 0x1f;
  let n;
  if (info < 24) n = info;
  else if (info === 24) n = r.uint(1);
  else if (info === 25) n = r.uint(2);
  else if (info === 26) n = r.uint(4);
  else if (info === 27) n = r.uint(8);
  else fail('malformed_evidence', 'unsupported measurement document encoding');
  const len = () => (typeof n === 'number' ? n : fail('malformed_evidence', 'measurement document length is too large'));
  switch (major) {
    case 0: return n;
    case 2: return r.take(len());
    case 3: return text(r.take(len()));
    case 4: {
      const out = [];
      for (let i = len(); i > 0; i--) out.push(cbor(r, depth + 1));
      return out;
    }
    case 5: {
      const out = {};
      for (let i = len(); i > 0; i--) {
        const key = cbor(r, depth + 1);
        if (typeof key !== 'string' || key in out) fail('malformed_evidence', 'invalid measurement document key');
        out[key] = cbor(r, depth + 1);
      }
      return out;
    }
    default: fail('malformed_evidence', 'unsupported measurement document encoding');
  }
}

function der(b, pos, limit = b.length) {
  if (pos + 2 > limit) fail('malformed_evidence', 'certificate is truncated');
  const tag = b[pos];
  let len = b[pos + 1];
  let start = pos + 2;
  if (len & 0x80) {
    const n = len & 0x7f;
    if (n === 0 || n > 4 || start + n > limit) fail('malformed_evidence', 'invalid certificate length');
    len = 0;
    for (let i = 0; i < n; i++) len = len * 256 + b[start + i];
    start += n;
  }
  if (start + len > limit) fail('malformed_evidence', 'certificate is truncated');
  return { tag, start, end: start + len, pos };
}
function derChildren(b, node) {
  const out = [];
  for (let p = node.start; p < node.end; ) {
    const child = der(b, p, node.end);
    out.push(child);
    p = child.end;
  }
  return out;
}
const derBytes = (b, node) => b.subarray(node.pos, node.end);
const derValue = (b, node) => b.subarray(node.start, node.end);

function derTime(b, node) {
  const s = text(derValue(b, node));
  const m = node.tag === 0x17 ? /^(\d\d)(\d\d)(\d\d)(\d\d)(\d\d)(\d\d)Z$/.exec(s) : /^(\d{4})(\d\d)(\d\d)(\d\d)(\d\d)(\d\d)Z$/.exec(s);
  if (!m) fail('malformed_evidence', 'invalid certificate time');
  const year = node.tag === 0x17 ? (Number(m[1]) >= 50 ? 1900 : 2000) + Number(m[1]) : Number(m[1]);
  return Date.UTC(year, Number(m[2]) - 1, Number(m[3]), Number(m[4]), Number(m[5]), Number(m[6]));
}

const OID_RSA_PSS = '2a864886f70d01010a';
const OID_BASIC_CONSTRAINTS = '551d13';

/** Minimal X.509 parse: only what chain and extension checks need. Any malformed input is `malformed_evidence`. */
function parseCertificate(input, what) {
  try {
    return parseCertificateUnchecked(input, what);
  } catch (e) {
    if (e instanceof AttestationError) throw e;
    fail('malformed_evidence', `${what} is not a certificate`, e);
  }
}

function parseCertificateUnchecked(input, what) {
  const b = certificateDer(input, what);
  const cert = der(b, 0);
  if (cert.tag !== 0x30 || cert.end !== b.length) fail('malformed_evidence', `${what} is not a certificate`);
  const [tbs, sigAlg, sig] = derChildren(b, cert);
  if (!tbs || !sigAlg || !sig || sig.tag !== 0x03) fail('malformed_evidence', `${what} is not a certificate`);
  const fields = derChildren(b, tbs);
  const offset = fields[0]?.tag === 0xa0 ? 1 : 0;
  const [, innerAlg, issuer, validity, subject, spki] = fields.slice(offset);
  if (!spki) fail('malformed_evidence', `${what} is not a certificate`);
  if (!equal(derBytes(b, innerAlg), derBytes(b, sigAlg))) fail('untrusted_hardware', `${what} has inconsistent signature algorithms`);
  const [notBefore, notAfter] = derChildren(b, validity);
  const extensions = new Map();
  const extBlock = fields.slice(offset + 6).find((f) => f.tag === 0xa3);
  if (extBlock) {
    for (const ext of derChildren(b, derChildren(b, extBlock)[0])) {
      const parts = derChildren(b, ext);
      extensions.set(toHex(derValue(b, parts[0])), derValue(b, parts[parts.length - 1]));
    }
  }
  const bc = extensions.get(OID_BASIC_CONSTRAINTS);
  let isCa = false;
  if (bc) {
    const seq = der(bc, 0);
    const first = derChildren(bc, seq)[0];
    isCa = first?.tag === 0x01 && bc[first.start] !== 0;
  }
  return {
    der: b,
    tbs: derBytes(b, tbs),
    signatureAlgorithm: toHex(derValue(b, derChildren(b, sigAlg)[0])),
    signature: b.subarray(sig.start + 1, sig.end),
    issuer: derBytes(b, issuer),
    subject: derBytes(b, subject),
    notBefore: derTime(b, notBefore),
    notAfter: derTime(b, notAfter),
    spki: derBytes(b, spki),
    extensions,
    isCa,
  };
}

/** Accepts DER bytes, hex DER, or one PEM certificate. */
function certificateDer(input, what) {
  if (typeof input === 'string') {
    const pem = /-----BEGIN CERTIFICATE-----([\s\S]*?)-----END CERTIFICATE-----/.exec(input);
    if (pem) return fromBase64(pem[1].replace(/\s+/g, ''), what);
    return fromHex(input, what);
  }
  if (input instanceof Uint8Array) {
    if (input[0] === 0x2d) return certificateDer(text(input), what);
    return input;
  }
  fail('malformed_evidence', `${what} is missing`);
}

function pemCertificates(pem) {
  return [...String(pem).matchAll(/-----BEGIN CERTIFICATE-----([\s\S]*?)-----END CERTIFICATE-----/g)].map((m) =>
    fromBase64(m[1].replace(/\s+/g, ''), 'certificate'),
  );
}

// ---------------------------------------------------------------------------
// AMD SEV-SNP: report, certificate chain, policy
// ---------------------------------------------------------------------------

/**
 * AMD root keys (ARK), one per processor family, as DER SubjectPublicKeyInfo.
 * Taken from https://kdsintf.amd.com/vcek/v1/{Milan,Genoa,Turin}/cert_chain.
 * These are the trust anchors: the ASK must be signed by one of them.
 */
const AMD_ROOT_KEYS = Object.freeze({
  Milan:
    'MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEA0Ld52RJOdeiJlqK2JdsVmD7FktuotWwX1fNgW41XY9Xz1HEhSUmhLz9Cu9DHRlvgJSNxbeYYsnJfvyjx1MfU0V5tkKiU1EesNFta1kTA0szNisdYc9isqk7mXT5+KfGRbfc4V/9zRIcE8jlHN61S1ju8X93+6dxDUrG2SzxqJ4BhqyYmUDruPXJSX4vUc01P7j98MpqOS95rORdGHeI52Naz5m2B+O+vjsC060d37jY9LFeuOP4Meri8qgfi2S5kKqg/aF6aPtuAZQVR7u3KFYXP59XmJgtcog05gmI0T/OitLhuzVvpZcLph0odh/1IPXqx3+MnjD97A7fXpqGd/y8KxX7jksTEzAOgbKAeam3lm+3yKIcTYMlsRMXPcjNbIvmsBykD//xSniusuHBkgnlENEWx1UcbQQrs+gVDkuVPhsnzIRNgYvM48Y+7LGiJYnrmE8xcrexekBxrva2V9TJQqnN3Q53kt5viQi3+gCfmkwC0F0tirIZbLkXPrPwzZ0M9eNxhIySb2npJfgnqz55I0u33wh4r0ZNQeTGfw03MBUtyuzGesGkcw+loqMaq1qR4tjGbPYxCvpCq7+OgpCCoMNit2uLo9M18fHz10lOMT8nWAUvRZFzteXCm+7PHdYPlmQwUw3LvenJ/ILXoQPHfbkH0CyPfhl1jWhJFZasCAwEAAQ==',
  Genoa:
    'MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEA3Cd95S/uFOuRIskW9vz9VDBF69NDQF79oRhL/L2PVQGhK3YdfEBgpF/JiwWFBsT/fXDhzA01p3LkcT/7LdjcRfKXjHl+0Qq/M4dZkh6QDoUeKzNBLDcBKDDGWo3v35NyrxbA1DnkYwUKU5AAk4P94tKXLp80oxt84ahyHoLmc/LqsGsp+oq1Bz4PPsYLwTG4iMKVaaT90/oZ4I8oibSru92vJhlqWO27d/Rxc3iUMyhNeGToOvgx/iUo4gGpG61NDpkEUvIzuKcaMx8IdTpWg2DF6SwF0IgVMffnvtJmA68BwJNWo1E4PLJdaPfBifcJpuBFwNVQIPQEVX3aP89HJSp8YbY9lySS6PlVEqTBBtaQmi4ATGmMR+n2K/e+JAhU2Gj7jIpJhOkdH9firQDnmlA2SFfJ/Cc0mGNzW9RmIhyOUnNFoclmkRhl3/AQU5Ys9Qsan1jT/EiyT+pCpmnA+y9edvhDCbOG8F2oxHGRdTBkylungrkXJGYiwGrR8kaiqv7NN8QhOBMqYjcbrkEr0f8QMKklIS5ruOfqlLMCBw8JLB3LkjpWgtD7OpxkzSsohN47Uom86RY6lp72g8eXHP1qYrnvhzaG1S70vw6OkbaaC9EjiH/uHgAJQGxon7u0Q7xgoREWA/e7JcBQwLg80Hq/sbRuqesxz7wBWSY254cCAwEAAQ==',
  Turin:
    'MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEAwaAriB7EIuVc4ZB1wD3YfDxL+9eyS7+izm0Jj3W772NINCWl8Bj3w/JD2ZjmbRxWdIq/4d9iarCKorXloJUB1jRdgxqccTx1aOoig4+2w1XhVVJT7K457wT5ZLNJgQaxqa9Etkwjd6+9sOhlCDE9l43kQ0R2BikVJa/uyyVOSwEk5w5tXKOuG9jvq6QtAMJasW38wlqRDaKEGtZ9VUgGon27ZuL4sTJuC/azz9/iQBw8kEilzOl95AiTkeY5jSEBDWbAqnZk5qlM7kISKG20kgQm14mhNKDI2p2oua+zuAG7i52epoRF2GfU0TYk/yf+vCNB2tnechFQuP2e8bLk95ZdqPi9/UWw4JXjtdEA4u2JYplSSUPQVAXKt6LVqujtJcM59JKr2u0XQ75KwxcMp15gSXhBfInvPAwuAY4dEwwGqT8oIg4esPHwEsmChhYeDIxPG9R4fx9O0q6p8Gb+HXlTiS47P9YNeOpidOUKzDl/S1OvyhDtSL8LJc24QATFydo/iD/KUdvFTRlD0crkAMkZLoWQ8hLDGc6BZJXsdd7Zf2e4UW3tI/1oh/2t23Ot3zyhTcv5gDbABu0LjVe98uRnS15SMwK//lJt9e5BqKvgABkSoABf+B4VFtPVEX0ygrYaFaI9i5ABrxnVBmzXpRb21iI1NlNCfOGUPIhVpWECAwEAAQ==',
});

const SNP_REPORT_LEN = 1184;
const SNP_SIGNED_LEN = 0x2a0;
const le32 = (b, o) => (b[o] | (b[o + 1] << 8) | (b[o + 2] << 16) | (b[o + 3] << 24)) >>> 0;
const le64 = (b, o) => new DataView(b.buffer, b.byteOffset + o, 8).getBigUint64(0, true);

function snpProduct(version, family, model) {
  if (version === 2) return null;
  if (family === 0x19) {
    if (model <= 0x0f) return 'Milan';
    if ((model >= 0x10 && model <= 0x1f) || (model >= 0xa0 && model <= 0xaf)) return 'Genoa';
  } else if (family === 0x1a && model <= 0x11) {
    return 'Turin';
  }
  fail('unsupported_platform', `unsupported AMD processor family ${family}/model ${model}`);
}

function snpTcb(b, o, turin) {
  return turin
    ? { fmc: b[o], bootloader: b[o + 1], tee: b[o + 2], snp: b[o + 3], microcode: b[o + 7] }
    : { bootloader: b[o], tee: b[o + 1], snp: b[o + 6], microcode: b[o + 7] };
}
const sameTcb = (a, b) => ['fmc', 'bootloader', 'tee', 'snp', 'microcode'].every((k) => a[k] === b[k]);

/** Parses an SNP attestation report. Fields are untrusted until the signature is verified. */
export function parseSnpReport(report) {
  if (!(report instanceof Uint8Array) || report.length !== SNP_REPORT_LEN) {
    fail('malformed_evidence', `SEV-SNP report must be ${SNP_REPORT_LEN} bytes`);
  }
  const version = le32(report, 0);
  if (version !== 2 && version !== 3) fail('unsupported_platform', `unsupported SEV-SNP report version ${version}`);
  const product = snpProduct(version, report[0x188], report[0x189]);
  const turin = product === 'Turin';
  return {
    version,
    product,
    policy: le64(report, 0x08),
    vmpl: le32(report, 0x30),
    signatureAlgorithm: le32(report, 0x34),
    platformInfo: le64(report, 0x40),
    keyInfo: le32(report, 0x48),
    reportData: report.slice(0x50, 0x90),
    measurement: report.slice(0x90, 0xc0),
    hostData: report.slice(0xc0, 0xe0),
    // Firmware-assigned id of this guest instance, constant for its lifetime.
    reportId: report.slice(0x140, 0x160),
    chipId: report.slice(0x1a0, 0x1e0),
    tcb: {
      current: snpTcb(report, 0x38, turin),
      reported: snpTcb(report, 0x180, turin),
      committed: snpTcb(report, 0x1e0, turin),
      launch: snpTcb(report, 0x1f0, turin),
    },
    signed: report.subarray(0, SNP_SIGNED_LEN),
    signatureR: report.subarray(0x2a0, 0x2a0 + 72),
    signatureS: report.subarray(0x2a0 + 72, 0x2a0 + 144),
  };
}

/** Invariant launch policy, matching what dstack's KMS requires before releasing keys. */
function checkSnpPolicy(r) {
  const bit = (v, n) => ((v >> BigInt(n)) & 1n) === 1n;
  const violation = (m) => fail('policy_violation', m);
  if (r.vmpl !== 0) violation('report was not generated at VMPL0');
  if (bit(r.policy, 19)) violation('guest policy allows debugging');
  if (bit(r.policy, 18)) violation('guest policy allows a migration agent');
  if (r.keyInfo & 0x2) violation('report masks the chip signing key');
  if (((r.keyInfo >> 2) & 0x7) !== 0) violation('report is not signed by a VCEK');
  if (r.signatureAlgorithm !== 1) violation('report does not use ECDSA P-384');
  if (!bit(r.policy, 16) && bit(r.platformInfo, 0)) violation('SMT is enabled but the guest policy forbids it');
  if (bit(r.policy, 23) && !bit(r.platformInfo, 3)) violation('guest policy requires RAPL disabled');
  if (bit(r.policy, 24) && !bit(r.platformInfo, 4)) violation('guest policy requires ciphertext hiding');
}

async function verifyRsaPss(spki, signature, message, what) {
  let ok = false;
  try {
    const key = await subtle().importKey('spki', spki, { name: 'RSA-PSS', hash: 'SHA-384' }, false, ['verify']);
    ok = await subtle().verify({ name: 'RSA-PSS', saltLength: 48 }, key, signature, message);
  } catch (e) {
    fail('untrusted_hardware', `${what} signature could not be checked`, e);
  }
  if (!ok) fail('untrusted_hardware', `${what} signature is invalid`);
}

// VCEK extensions (AMD "Versioned Chip Endorsement Key Certificate" spec).
const OID_VCEK = {
  bootloader: '2b060104019c78010301',
  tee: '2b060104019c78010302',
  snp: '2b060104019c78010303',
  microcode: '2b060104019c78010308',
  fmc: '2b060104019c78010309',
  hwid: '2b060104019c780104',
};

function vcekInteger(cert, key) {
  const value = cert.extensions.get(OID_VCEK[key]);
  if (!value) return undefined;
  const node = der(value, 0);
  if (node.tag !== 0x02 || node.end !== value.length) fail('untrusted_hardware', 'VCEK has a malformed TCB extension');
  let n = 0;
  for (const byte of derValue(value, node)) n = n * 256 + byte;
  return n;
}

/**
 * Verifies ARK -> ASK -> VCEK -> report and that the VCEK was issued for this
 * chip at the reported firmware level. Returns nothing; throws on failure.
 */
async function verifySnpSignatureChain(reportBytes, r, collateral, now) {
  const ask = parseCertificate(collateral.ask, 'AMD ASK certificate');
  const vcek = parseCertificate(collateral.vcek, 'AMD VCEK certificate');
  for (const [cert, what] of [[ask, 'ASK'], [vcek, 'VCEK']]) {
    if (cert.signatureAlgorithm !== OID_RSA_PSS) fail('untrusted_hardware', `AMD ${what} is not signed with RSA-PSS`);
    if (now < cert.notBefore - 5 * 60_000 || now > cert.notAfter) fail('untrusted_hardware', `AMD ${what} certificate is not valid now`);
  }
  if (!ask.isCa) fail('untrusted_hardware', 'AMD ASK is not a certificate authority');
  if (!equal(vcek.issuer, ask.subject)) fail('untrusted_hardware', 'VCEK was not issued by the supplied ASK');

  // Report v2 does not name its processor family, so try each pinned root.
  const candidates = r.product ? [r.product] : Object.keys(AMD_ROOT_KEYS);
  let rooted = false;
  for (const product of candidates) {
    try {
      await verifyRsaPss(fromBase64(AMD_ROOT_KEYS[product]), ask.signature, ask.tbs, 'AMD ASK');
      rooted = true;
      break;
    } catch (e) {
      if (candidates.length === 1) throw e;
    }
  }
  if (!rooted) fail('untrusted_hardware', 'AMD ASK does not chain to a pinned AMD root key');
  await verifyRsaPss(ask.spki, vcek.signature, vcek.tbs, 'AMD VCEK');

  // The VCEK must be the one AMD derived for this chip and reported TCB.
  const hwid = vcek.extensions.get(OID_VCEK.hwid);
  if (!hwid) fail('untrusted_hardware', 'VCEK does not name a chip');
  let chip = hwid;
  if (chip.length !== 64 && chip.length !== 8 && chip[0] === 0x04) chip = derValue(chip, der(chip, 0));
  if (chip.length < 8 || !equal(chip, r.chipId.subarray(0, chip.length)) || !isZero(r.chipId.subarray(chip.length))) {
    fail('untrusted_hardware', 'VCEK was issued for a different chip');
  }
  for (const key of ['bootloader', 'tee', 'snp', 'microcode', 'fmc']) {
    const expected = r.tcb.reported[key];
    if (expected === undefined) continue;
    if (vcekInteger(vcek, key) !== expected) fail('untrusted_hardware', `VCEK ${key} version does not match the report`);
  }

  // Report signature: r and s are 72-byte little-endian; WebCrypto wants
  // 48-byte big-endian r || s.
  if (!isZero(r.signatureR.subarray(48)) || !isZero(r.signatureS.subarray(48))) {
    fail('untrusted_hardware', 'report signature is malformed');
  }
  const p1363 = concat(r.signatureR.slice(0, 48).reverse(), r.signatureS.slice(0, 48).reverse());
  let ok = false;
  try {
    const key = await subtle().importKey('spki', vcek.spki, { name: 'ECDSA', namedCurve: 'P-384' }, false, ['verify']);
    ok = await subtle().verify({ name: 'ECDSA', hash: 'SHA-384' }, key, p1363, reportBytes.subarray(0, SNP_SIGNED_LEN));
  } catch (e) {
    fail('untrusted_hardware', 'report signature could not be checked', e);
  }
  if (!ok) fail('untrusted_hardware', 'report signature is invalid');
}

/** AMD Key Distribution Service. It sends no CORS headers, so browsers need server-supplied collateral. */
export const AMD_KDS_BASE_URL = 'https://kdsintf.amd.com';

/** URL of the VCEK certificate AMD issues for a report's chip and reported firmware. */
export function amdKdsVcekUrl(parsedReport, kdsBaseUrl = AMD_KDS_BASE_URL) {
  const r = parsedReport;
  if (!r.product) fail('collateral_unavailable', 'report does not name its processor family; supply collateral');
  const t = r.tcb.reported;
  const base = `${kdsBaseUrl.replace(/\/$/, '')}/vcek/v1/${r.product}`;
  return r.product === 'Turin'
    ? `${base}/${toHex(r.chipId.subarray(0, 8))}?fmcSPL=${t.fmc}&blSPL=${t.bootloader}&teeSPL=${t.tee}&snpSPL=${t.snp}&ucodeSPL=${t.microcode}`
    : `${base}/${toHex(r.chipId)}?blSPL=${t.bootloader}&teeSPL=${t.tee}&snpSPL=${t.snp}&ucodeSPL=${t.microcode}`;
}

async function fetchBytes(url, timeoutMs, fetchImpl) {
  const res = await fetchImpl(url, { signal: AbortSignal.timeout(timeoutMs) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return new Uint8Array(await res.arrayBuffer());
}

/** Fetches the ASK and VCEK for a report from AMD KDS. */
async function fetchAmdCollateral(r, { kdsBaseUrl = AMD_KDS_BASE_URL, timeoutMs, fetchImpl }) {
  try {
    const [vcek, chain] = await Promise.all([
      fetchBytes(amdKdsVcekUrl(r, kdsBaseUrl), timeoutMs, fetchImpl),
      fetchBytes(`${kdsBaseUrl.replace(/\/$/, '')}/vcek/v1/${r.product}/cert_chain`, timeoutMs, fetchImpl),
    ]);
    const certs = pemCertificates(text(chain));
    if (certs.length < 1) throw new Error('AMD cert_chain is empty');
    return { vcek, ask: certs[0] };
  } catch (e) {
    if (e instanceof AttestationError) throw e;
    fail(
      'collateral_unavailable',
      `could not fetch AMD endorsement certificates from ${new URL(kdsBaseUrl).host} (${e?.message ?? e}). Browsers cannot reach AMD KDS; the deployment must supply collateral`,
      e,
    );
  }
}

const GUID_VCEK = '63da758de6644564adc5f4b93be8accd';
const GUID_ASK = '4ab7b379bbac4fe4a02f05aef327c782';

/**
 * ASK and VCEK from the certificate blobs an evidence envelope carries: either
 * the two certificates themselves, or one blob holding the kernel's SNP
 * certificate table (24-byte entries of GUID, offset and length, ended by a
 * zero entry). Returns null when it is neither, so the caller moves on.
 */
function envelopeCollateral(certChain) {
  if (certChain.length === 2) return { ask: certChain[0], vcek: certChain[1] };
  if (certChain.length !== 1) return null;
  const blob = certChain[0];
  const found = {};
  for (let at = 0; at + 24 <= blob.length; at += 24) {
    const guid = toHex(blob.subarray(at, at + 16));
    if (/^0+$/.test(guid)) break;
    const offset = le32(blob, at + 16);
    const length = le32(blob, at + 20);
    if (length === 0 || offset + length > blob.length) return null;
    if (guid === GUID_VCEK) found.vcek = blob.subarray(offset, offset + length);
    if (guid === GUID_ASK) found.ask = blob.subarray(offset, offset + length);
  }
  return found.vcek && found.ask ? found : null;
}

/** Picks ASK and VCEK out of whatever the response or the envelope carried. */
function normalizeCollateral(source) {
  if (!source) return null;
  if (source.vcek && source.ask) return { vcek: source.vcek, ask: source.ask };
  return null;
}

// ---------------------------------------------------------------------------
// AMD SEV-SNP: launch measurement and app identity
// ---------------------------------------------------------------------------

const MAX_VCPUS = 512;
const MAX_OVMF_SECTIONS = 64;
const MAX_OVMF_PAGES = 0x10000;
const VMSA_GPA = 0x0000fffffffff000n;
const GUID_HASH_TABLE = fromHexStatic('06d63894224fc94cb479a793d411fd21');
const GUID_KERNEL = fromHexStatic('3794e74dd2ab7f42b835d5b172d2045b');
const GUID_INITRD = fromHexStatic('31f7ba442f3ad74b9af141e29169781d');
const GUID_CMDLINE = fromHexStatic('d82dd09720bd944caa78e7714d36ab2a');
function fromHexStatic(hex) {
  return Uint8Array.from(hex.match(/../g), (x) => parseInt(x, 16));
}

/** SNP launch digest context: SHA-384 chain over PAGE_INFO structures. */
class LaunchDigest {
  constructor(seed) {
    this.ld = seed;
  }
  async update(pageType, gpa, contents) {
    const buf = new Uint8Array(0x70);
    buf.set(this.ld, 0);
    buf.set(contents, 48);
    const view = new DataView(buf.buffer);
    view.setUint16(96, 0x70, true);
    buf[98] = pageType;
    view.setBigUint64(104, BigInt(gpa), true);
    this.ld = await sha384(buf);
  }
}
const ZERO48 = new Uint8Array(48);

function vcpuSignature(vcpuType) {
  const sig = (family, model, stepping) => {
    const familyLow = family > 0xf ? 0xf : family;
    const familyHigh = family > 0xf ? (family - 0xf) & 0xff : 0;
    return ((familyHigh << 20) | (((model >> 4) & 0xf) << 16) | (familyLow << 8) | ((model & 0xf) << 4) | (stepping & 0xf)) >>> 0;
  };
  switch (String(vcpuType).trim().toLowerCase()) {
    case 'epyc': case 'epyc-v1': case 'epyc-v2': case 'epyc-ibpb': case 'epyc-v3': case 'epyc-v4':
      return sig(23, 1, 2);
    case 'epyc-rome': case 'epyc-rome-v1': case 'epyc-rome-v2': case 'epyc-rome-v3':
      return sig(23, 49, 0);
    case 'epyc-milan': case 'epyc-milan-v1': case 'epyc-milan-v2':
      return sig(25, 1, 1);
    case 'epyc-genoa': case 'epyc-genoa-v1':
      return sig(25, 17, 0);
    default:
      fail('measurement_mismatch', `unknown vcpu_type ${JSON.stringify(vcpuType)}`);
  }
}

function vmsaPage(eip, vcpuSig, sevFeatures) {
  const page = new Uint8Array(4096);
  const v = new DataView(page.buffer);
  const seg = (off, selector, attrib, limit, base) => {
    v.setUint16(off, selector, true);
    v.setUint16(off + 2, attrib, true);
    v.setUint32(off + 4, limit, true);
    v.setBigUint64(off + 8, BigInt(base), true);
  };
  const u64 = (off, value) => v.setBigUint64(off, BigInt(value), true);
  seg(0x000, 0, 0x0093, 0xffff, 0);
  seg(0x010, 0xf000, 0x009b, 0xffff, (eip & 0xffff0000) >>> 0);
  seg(0x020, 0, 0x0093, 0xffff, 0);
  seg(0x030, 0, 0x0093, 0xffff, 0);
  seg(0x040, 0, 0x0093, 0xffff, 0);
  seg(0x050, 0, 0x0093, 0xffff, 0);
  seg(0x060, 0, 0x0000, 0xffff, 0);
  seg(0x070, 0, 0x0082, 0xffff, 0);
  seg(0x080, 0, 0x0000, 0xffff, 0);
  seg(0x090, 0, 0x008b, 0xffff, 0);
  u64(0x0d0, 0x1000);
  u64(0x148, 0x40);
  u64(0x158, 0x10);
  u64(0x160, 0x400);
  u64(0x168, 0xffff0ff0);
  u64(0x170, 0x2);
  u64(0x178, eip & 0xffff);
  u64(0x268, 0x0007040600070406n);
  u64(0x310, vcpuSig);
  u64(0x3b0, sevFeatures);
  u64(0x3e8, 0x1);
  v.setUint32(0x408, 0x1f80, true);
  v.setUint16(0x410, 0x037f, true);
  return page;
}

async function sevHashesPage(kernelHash, initrdHash, cmdline, pageOffset) {
  const entry = (guid, hash) => {
    const e = new Uint8Array(50);
    e.set(guid, 0);
    e[16] = 50;
    e.set(hash, 18);
    return e;
  };
  const cmdlineHash = await sha256(concat(utf8(cmdline), new Uint8Array(1)));
  const table = new Uint8Array(168);
  table.set(GUID_HASH_TABLE, 0);
  table[16] = 168;
  table.set(entry(GUID_CMDLINE, cmdlineHash), 18);
  table.set(entry(GUID_INITRD, initrdHash), 68);
  table.set(entry(GUID_KERNEL, kernelHash), 118);
  if (pageOffset + 176 > 4096) fail('measurement_mismatch', 'SEV hash table overflows its page');
  const page = new Uint8Array(4096);
  page.set(table, pageOffset);
  return page;
}

const safeInt = (v, what) => (Number.isSafeInteger(v) && v >= 0 ? v : fail('malformed_evidence', `${what} is not a valid integer`));
const onlyKeys = (obj, allowed, what) => {
  if (!obj || typeof obj !== 'object' || Array.isArray(obj)) fail('malformed_evidence', `${what} is not an object`);
  for (const key of Object.keys(obj)) if (!allowed.includes(key)) fail('malformed_evidence', `${what} has unknown field ${key}`);
};

/**
 * Recomputes the SNP launch measurement from the launch inputs the VM declares
 * and returns it with the OS image hash those inputs commit to. The inputs are
 * untrusted: the caller must compare the result with the signed measurement.
 */
export async function computeSnpLaunch(vmConfigJson) {
  let vmConfig;
  try {
    vmConfig = JSON.parse(vmConfigJson);
  } catch (e) {
    fail('malformed_evidence', 'vm_config is not JSON', e);
  }
  const osImageHash = hexOfLength(vmConfig?.os_image_hash ?? '', 32, 'vm_config.os_image_hash', 'malformed_evidence');
  if (typeof vmConfig.sev_snp_measurement !== 'string') fail('malformed_evidence', 'vm_config has no sev_snp_measurement');
  let doc;
  try {
    doc = JSON.parse(vmConfig.sev_snp_measurement);
  } catch (e) {
    fail('malformed_evidence', 'sev_snp_measurement is not JSON', e);
  }
  onlyKeys(doc, ['checksum_file', 'measurement', 'vcpus', 'vcpu_type', 'guest_features'], 'sev_snp_measurement');
  const checksumFile = fromBase64(doc.checksum_file, 'checksum_file');
  const material = fromBase64(doc.measurement, 'measurement');

  // The OS image identity is sha256(sha256sum.txt), and that file must list the
  // exact measurement material used below.
  if (toHex(await sha256(checksumFile)) !== osImageHash) fail('measurement_mismatch', 'os_image_hash does not match the image checksum file');
  const lines = text(checksumFile).split('\n').map((l) => /^([0-9a-f]{64}) {2}(\S.*)$/.exec(l)).filter(Boolean);
  const listed = lines.filter((m) => m[2] === 'measurement.snp.cbor');
  if (listed.length !== 1 || listed[0][1] !== toHex(await sha256(material))) {
    fail('measurement_mismatch', 'measurement material is not the one the OS image lists');
  }

  const r = new Reader(material, 'measurement document');
  const m = cbor(r);
  r.done();
  onlyKeys(m, ['version', 'cmdline', 'ovmf_hash', 'kernel_hash', 'initrd_hash', 'hashes_table_gpa', 'reset_eip', 'ovmf_sections'], 'measurement document');
  if (m.version !== 3) fail('malformed_evidence', 'unsupported measurement document version');
  const bytesOf = (v, n, what) => (v instanceof Uint8Array && v.length === n ? v : fail('malformed_evidence', `${what} must be ${n} bytes`));
  const ovmfHash = bytesOf(m.ovmf_hash, 48, 'ovmf_hash');
  const kernelHash = bytesOf(m.kernel_hash, 32, 'kernel_hash');
  const initrdHash = bytesOf(m.initrd_hash, 32, 'initrd_hash');
  if (typeof m.cmdline !== 'string') fail('malformed_evidence', 'cmdline is missing');
  const cmdline = m.cmdline.trim();
  const rootfs = cmdline.split(/\s+/).filter((p) => p.startsWith('dstack.rootfs_hash='));
  if (rootfs.length !== 1 || !/^dstack\.rootfs_hash=[0-9a-fA-F]{64}$/.test(rootfs[0])) {
    fail('measurement_mismatch', 'measured command line must pin exactly one dstack.rootfs_hash');
  }
  const hashesGpa = safeInt(m.hashes_table_gpa, 'hashes_table_gpa');
  const resetEip = safeInt(m.reset_eip, 'reset_eip');
  if (hashesGpa === 0 || resetEip === 0 || resetEip > 0xffffffff) fail('malformed_evidence', 'invalid firmware metadata');
  const vcpus = safeInt(doc.vcpus, 'vcpus');
  if (vcpus === 0 || vcpus > MAX_VCPUS) fail('malformed_evidence', 'invalid vcpu count');
  // SNPActive only. Any other SEV feature bit (DebugSwap exposes guest state to
  // the host) is not something dstack launches, so refuse it.
  if (doc.guest_features !== 1) fail('policy_violation', 'guest launched with unexpected SEV features');
  const vcpuSig = vcpuSignature(doc.vcpu_type);
  if (!Array.isArray(m.ovmf_sections) || m.ovmf_sections.length === 0 || m.ovmf_sections.length > MAX_OVMF_SECTIONS) {
    fail('malformed_evidence', 'invalid firmware section list');
  }

  const ld = new LaunchDigest(ovmfHash);
  let pages = 0;
  let sawHashes = false;
  for (const section of m.ovmf_sections) {
    onlyKeys(section, ['gpa', 'size', 'type'], 'firmware section');
    const gpa = safeInt(section.gpa, 'section gpa');
    const size = safeInt(section.size, 'section size');
    if (size === 0) fail('malformed_evidence', 'empty firmware section');
    pages += Math.ceil(size / 4096);
    if (pages > MAX_OVMF_PAGES) fail('malformed_evidence', 'too many measured firmware pages');
    switch (section.type) {
      case 1: // SNP_SEC_MEMORY
      case 4: // SVSM_CAA
        for (let off = 0; off < size; off += 4096) await ld.update(0x03, gpa + off, ZERO48);
        break;
      case 2: // SNP_SECRETS
        await ld.update(0x05, gpa, ZERO48);
        break;
      case 3: // CPUID
        await ld.update(0x06, gpa, ZERO48);
        break;
      case 0x10: // SNP_KERNEL_HASHES
        sawHashes = true;
        await ld.update(0x01, gpa, await sha384(await sevHashesPage(kernelHash, initrdHash, cmdline, hashesGpa & 0xfff)));
        break;
      default:
        fail('malformed_evidence', `unknown firmware section type ${section.type}`);
    }
  }
  if (!sawHashes) fail('measurement_mismatch', 'firmware metadata has no kernel-hashes section');
  const bsp = await sha384(vmsaPage(0xfffffff0, vcpuSig, 1));
  const ap = await sha384(vmsaPage(resetEip, vcpuSig, 1));
  for (let i = 0; i < vcpus; i++) await ld.update(0x02, VMSA_GPA, i === 0 ? bsp : ap);
  return { measurement: ld.ld, osImageHash, cmdline, vcpus, mrConfig: typeof vmConfig.mr_config === 'string' ? vmConfig.mr_config : null };
}

const MR_CONFIG_DOMAIN = utf8('dstack-mr-config-v3:');
const MR_CONFIG_FIELDS = ['version', 'app_id', 'compose_hash', 'gpu_policy_hash', 'key_provider', 'key_provider_id', 'instance_id', 'init_script_hashes'];

/** Checks that HOST_DATA binds this exact app-identity document and returns its fields. */
async function bindMrConfig(document, hostData) {
  const expected = await sha256(concat(MR_CONFIG_DOMAIN, new Uint8Array(1), utf8(document)));
  if (!equal(expected, hostData)) fail('identity_mismatch', 'app identity document is not the one bound by the report');
  let doc;
  try {
    doc = JSON.parse(document);
  } catch (e) {
    fail('malformed_evidence', 'app identity document is not JSON', e);
  }
  onlyKeys(doc, MR_CONFIG_FIELDS, 'app identity document');
  if (doc.version !== 3) fail('malformed_evidence', 'unsupported app identity document version');
  return {
    appId: doc.app_id == null ? null : hexOfLength(doc.app_id, 20, 'app_id', 'malformed_evidence'),
    composeHash: hexOfLength(doc.compose_hash ?? '', 32, 'compose_hash', 'malformed_evidence'),
    instanceId: doc.instance_id == null ? null : hexOfLength(doc.instance_id, 20, 'instance_id', 'malformed_evidence'),
    keyProvider: doc.key_provider,
  };
}

/**
 * @typedef {Object} SnpVerification
 * @property {'sev-snp'} platform
 * @property {string} appId - dstack app id bound by the report.
 * @property {string} composeHash - Hash of the app-compose document the VM runs.
 * @property {string} osImageHash - dstack OS image identity the launch measurement reproduces.
 * @property {string|null} instanceId
 * @property {string} deviceId - sha256 of the chip id (the dstack device id).
 * @property {string} reportId - Firmware-assigned id of this guest instance; equal across reports from one running VM.
 * @property {string} measurement - Signed launch measurement, hex.
 * @property {string} reportData - Signed report data, hex.
 * @property {string|null} product - AMD processor family.
 * @property {{current: Object, reported: Object, committed: Object, launch: Object}} tcb
 * @property {'UpToDate'|'OutOfDate'} tcbStatus
 */

/**
 * Verifies decoded SEV-SNP evidence offline: hardware chain, report signature,
 * report-data binding, launch policy, launch measurement and app identity.
 * It does not consult the chain: see `verifyAttestation` for governance.
 *
 * @param {Object} evidence - From `decodeAttestation`.
 * @param {Object} options
 * @param {Uint8Array} options.expectedReportData - The 64 bytes the report must carry.
 * @param {{vcek: Uint8Array|string, ask: Uint8Array|string}} options.collateral - AMD VCEK and ASK certificates (DER, hex or PEM).
 * @param {number} [options.now] - Milliseconds since the epoch, for certificate validity.
 * @returns {Promise<SnpVerification>}
 */
export async function verifySnpEvidence(evidence, { expectedReportData, collateral, now = Date.now() } = {}) {
  if (evidence?.platform !== 'sev-snp') fail('unsupported_platform', 'evidence is not AMD SEV-SNP');
  if (!(expectedReportData instanceof Uint8Array) || expectedReportData.length !== 64) {
    fail('invalid_argument', 'expectedReportData must be 64 bytes');
  }
  const chain = normalizeCollateral(collateral);
  if (!chain) fail('collateral_unavailable', 'AMD VCEK and ASK certificates are required');
  const r = parseSnpReport(evidence.report);

  await verifySnpSignatureChain(evidence.report, r, chain, now);
  // Everything read from the report below is now hardware-signed.
  if (!equal(r.reportData, expectedReportData)) {
    fail('stale_evidence', 'report does not commit to this request (replayed or substituted evidence)');
  }
  checkSnpPolicy(r);

  const launch = await computeSnpLaunch(evidence.config);
  if (!equal(launch.measurement, r.measurement)) {
    fail('measurement_mismatch', 'declared firmware, kernel and command line do not reproduce the signed launch measurement');
  }
  // The VM's own configuration must carry the same document the envelope does.
  if (launch.mrConfig !== evidence.mrConfig) {
    fail('identity_mismatch', launch.mrConfig === null ? 'vm_config carries no app identity document' : 'attestation carries two different app identity documents');
  }
  const identity = await bindMrConfig(evidence.mrConfig, r.hostData);
  if (!identity.appId) fail('identity_mismatch', 'report does not pin an app id');
  if (identity.keyProvider !== 'kms') fail('identity_mismatch', 'app keys are not provided by the dstack KMS');

  const t = r.tcb;
  return {
    platform: 'sev-snp',
    appId: identity.appId,
    composeHash: identity.composeHash,
    osImageHash: launch.osImageHash,
    instanceId: identity.instanceId,
    deviceId: toHex(await sha256(r.chipId)),
    reportId: toHex(r.reportId),
    measurement: toHex(r.measurement),
    reportData: toHex(r.reportData),
    product: r.product,
    tcb: t,
    tcbStatus: sameTcb(t.current, t.reported) && sameTcb(t.committed, t.reported) && sameTcb(t.launch, t.reported) ? 'UpToDate' : 'OutOfDate',
  };
}

function checkTcbPolicy(result, policy) {
  if (policy.requireUpToDateTcb && result.tcbStatus !== 'UpToDate') {
    fail('policy_violation', 'platform firmware is not up to date');
  }
  for (const [key, min] of Object.entries(policy.minTcb ?? {})) {
    const actual = result.tcb.reported[key];
    if (actual === undefined || actual < min) fail('policy_violation', `platform ${key} version ${actual} is below the required ${min}`);
  }
}

// ---------------------------------------------------------------------------
// On-chain governance
// ---------------------------------------------------------------------------

const SELECTOR_ALLOWED_COMPOSE_HASHES = '2f6622e5'; // DstackApp.allowedComposeHashes(bytes32)
const SELECTOR_ALLOWED_OS_IMAGES = '9a4e1d18'; // DstackKms.allowedOsImages(bytes32)
const TRUE_WORD = '0x' + '0'.repeat(63) + '1';

/**
 * Asks the chain whether `hash32` is allowlisted, trying each endpoint until
 * one answers. Transport, HTTP and RPC-level failures fall through; a
 * successful `eth_call` is authoritative and never retried elsewhere.
 */
async function allowedOnChain(rpcUrls, contract, selector, hash32, timeoutMs, fetchImpl) {
  const body = JSON.stringify({
    jsonrpc: '2.0',
    id: 1,
    method: 'eth_call',
    params: [{ to: '0x' + hexOfLength(contract, 20, 'contract address'), data: '0x' + selector + hash32 }, 'latest'],
  });
  const failures = [];
  for (const rpcUrl of rpcUrls) {
    const host = new URL(rpcUrl).host;
    try {
      const res = await fetchImpl(rpcUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body,
        signal: AbortSignal.timeout(timeoutMs),
      });
      if (!res.ok) {
        failures.push(`${host}: HTTP ${res.status}`);
        continue;
      }
      const json = await res.json();
      if (json?.error !== undefined || typeof json?.result !== 'string') {
        failures.push(`${host}: RPC error`);
        continue;
      }
      return json.result === TRUE_WORD;
    } catch (e) {
      failures.push(`${host}: ${e?.message ?? e}`);
    }
  }
  fail(
    'governance_unavailable',
    `no RPC endpoint answered the on-chain governance check (${failures.join('; ')}). Retry shortly or pass your own Base RPC as attestation.rpcUrls`,
  );
}

// ---------------------------------------------------------------------------
// TLS channel binding (Node.js)
// ---------------------------------------------------------------------------

const MAX_EVIDENCE_FILE_BYTES = 1024 * 1024;
const TLS_BINDING_MODES = ['auto', 'require', 'strict', 'off'];
/** `code` on the error a pinned connection raises when the server's key is not the attested one. */
export const TLS_PIN_MISMATCH = 'ERR_LIT_TLS_PIN_MISMATCH';

/** Node built-ins, named at run time so bundlers do not resolve them for the browser. */
async function nodeModule(name) {
  // A browser would log a failed load of `node:https` to the console on every
  // verification, so do not ask one.
  if (!globalThis.process?.versions?.node) throw new Error('Node.js built-in modules are not available in this runtime');
  const specifier = ['node', name].join(':');
  return import(/* webpackIgnore: true */ /* @vite-ignore */ specifier);
}

/**
 * Fetches one file from the deployment's `/evidences/` directory over its own
 * new TLS connection and returns it with the certificate that connection
 * presented. dstack-ingress routes `/evidences` by the first bytes of a
 * connection, so the request must be the first on its connection: a pooled
 * connection that already carried an API request would reach the API instead.
 * `deadline` bounds the whole exchange, not just inactivity.
 */
function evidenceRequest(https, url, path, ctx, deadline) {
  return nodeGet(https, url, `/evidences/${path}`, ctx, deadline, MAX_EVIDENCE_FILE_BYTES);
}

/**
 * One GET on a connection of its own, returning the certificate that
 * connection presented. While a deployment cuts over, two VMs serve the
 * hostname, each with its own TLS key, and every new connection reaches either
 * one: the key is how a caller tells which VM answered.
 */
function nodeGet(https, url, path, ctx, deadline, maxBytes) {
  return new Promise((resolve, reject) => {
    let timer;
    const settle = (fn, value) => {
      clearTimeout(timer);
      fn(value);
    };
    const req = https.get(
      {
        host: url.hostname,
        port: Number(url.port) || 443,
        servername: url.hostname,
        path,
        agent: false,
        headers: { Connection: 'close' },
        ...(ctx.tls ?? {}),
      },
      (res) => {
        const certificate = res.socket.getPeerCertificate?.(false)?.raw;
        const chunks = [];
        let size = 0;
        res.on('data', (chunk) => {
          size += chunk.length;
          if (size > maxBytes) req.destroy(new Error('response is too large'));
          else chunks.push(chunk);
        });
        res.on('end', () => {
          const body = new Uint8Array(size);
          let offset = 0;
          for (const chunk of chunks) {
            body.set(chunk, offset);
            offset += chunk.length;
          }
          settle(resolve, { status: res.statusCode, body, certificate: certificate ? new Uint8Array(certificate) : null });
        });
        res.on('error', (e) => settle(reject, e));
      },
    );
    timer = setTimeout(() => req.destroy(new Error('timed out')), Math.max(1, deadline - Date.now()));
    req.on('error', (e) => settle(reject, e));
  });
}

// How often one request is repeated to reach the VM the others reached. Enough
// that an even split between two VMs almost never exhausts it.
const SAME_VM_ATTEMPTS = 10;

/** Hex SHA-256 of the public key in a connection's certificate, or null. */
async function connectionKey(certificate) {
  if (!certificate) return null;
  try {
    return toHex(await sha256(parseCertificate(certificate, 'TLS certificate').spki));
  } catch {
    return null;
  }
}

/**
 * Checks that the TLS certificate `origin` presents was issued to a key held
 * inside the attested VM. dstack-ingress (2.6 and later) publishes, under
 * `/evidences/`, the certificate it obtained and platform evidence whose
 * report data commits to it. This verifies that evidence the same way as the
 * main attestation, requires it to come from the **same running VM** as
 * `expected` (the same app and release is not enough: another replica would
 * pass that), and compares the certificate with the one each connection used.
 *
 * `status: 'verified'` covers the connections made here. `tlsKeySha256` is the
 * SHA-256 of the attested public key (SubjectPublicKeyInfo): pin later
 * connections to it with `createPinnedFetch`, which is what binds the
 * requests that carry your data. The Core SDK client does this.
 *
 * Node.js only: browsers cannot see a connection's certificate.
 *
 * @param {string} origin
 * @param {{appId: string, composeHash: string, osImageHash: string, reportId: string}} expected - From the main verification.
 * @param {Object} [options] - `timeoutMs`, `collateral`, `kdsBaseUrl`, `fetch`, `now`, `tls`, and
 *   `tlsKeySha256`: the TLS key of the connection the main attestation came over, when known.
 * @returns {Promise<{status: 'verified', tlsKeySha256: string} | {status: 'unavailable'|'unsupported'}>}
 *   `'unsupported'` when this runtime cannot inspect TLS certificates; `'unavailable'` when the deployment
 *   publishes no platform evidence. Throws `tls_binding_failed` when it publishes evidence that does not bind.
 */
export async function verifyTlsBinding(origin, expected, options = {}) {
  const ctx = {
    timeoutMs: options.timeoutMs ?? 20_000,
    fetchImpl: options.fetch ?? options.fetchImpl ?? ((...args) => globalThis.fetch(...args)),
    kdsBaseUrl: options.kdsBaseUrl,
    collateral: options.collateral,
    now: options.now ?? Date.now(),
    tls: options.tls,
  };
  for (const key of ['appId', 'composeHash', 'osImageHash', 'reportId']) {
    if (typeof expected?.[key] !== 'string' || !expected[key]) fail('invalid_argument', `expected.${key} is required to bind TLS`);
  }
  const url = new URL(origin);
  if (url.protocol !== 'https:') return { status: 'unsupported' };
  let https;
  try {
    https = await nodeModule('https');
  } catch {
    return { status: 'unsupported' };
  }
  const deadline = Date.now() + ctx.timeoutMs;
  // Every file must come from the VM that answered the main attestation.
  // While two VMs serve the hostname a connection reaches either, so a file
  // answered under another TLS key is asked for again. Without a key to aim
  // for, the first file's connection sets it.
  let aim = options.tlsKeySha256 ?? null;
  const get = async (path) => {
    let file;
    for (let attempt = 0; attempt < SAME_VM_ATTEMPTS; attempt++) {
      file = await evidenceRequest(https, url, path, ctx, deadline).catch((e) =>
        fail('tls_binding_failed', `could not fetch /evidences/${path} (${e?.message ?? e})`, e),
      );
      const key = await connectionKey(file.certificate);
      if (!aim) aim = key;
      if (!key || key === aim) break;
    }
    return file;
  };

  const attestation = await get('attestation.json');
  if (attestation.status === 404) return { status: 'unavailable' };
  const [manifest, certPem] = await Promise.all([get('sha256sum.txt'), get(`cert-${url.hostname}.pem`)]);
  for (const [file, name] of [[attestation, 'attestation.json'], [manifest, 'sha256sum.txt'], [certPem, `cert-${url.hostname}.pem`]]) {
    if (file.status !== 200) fail('tls_binding_failed', `/evidences/${name} answered HTTP ${file.status}`);
  }

  let evidenceHex;
  try {
    evidenceHex = JSON.parse(text(attestation.body)).attestation;
  } catch (e) {
    fail('tls_binding_failed', 'evidence attestation is not JSON', e);
  }
  // The ingress pads sha256(sha256sum.txt) with zeros to the 64 report-data bytes.
  const reportData = new Uint8Array(64);
  reportData.set(await sha256(manifest.body));
  let bound;
  try {
    const evidence = decodeAttestation(fromHex(evidenceHex, 'evidence attestation'));
    const r = parseSnpReport(evidence.report ?? new Uint8Array());
    // The certificate report is older than the fresh one. Usually the same
    // chip and firmware signed both and the same collateral verifies it; if
    // the firmware changed in between, it needs the VCEK for its own TCB.
    const sources = [() => ctx.collateral, () => fetchAmdCollateral(r, ctx)];
    let lastError;
    for (const source of sources) {
      try {
        const collateral = await source();
        if (!collateral) continue;
        bound = await verifySnpEvidence(evidence, { expectedReportData: reportData, collateral, now: ctx.now });
        break;
      } catch (e) {
        if (!(e instanceof AttestationError) || e.code !== 'untrusted_hardware') throw e;
        lastError = e;
      }
    }
    if (!bound) throw lastError ?? new AttestationError('collateral_unavailable', 'AMD VCEK and ASK certificates are required');
  } catch (e) {
    fail('tls_binding_failed', `certificate evidence did not verify: ${e?.message ?? e}`, e);
  }
  if (bound.appId !== expected.appId || bound.composeHash !== expected.composeHash || bound.osImageHash !== expected.osImageHash) {
    fail('tls_binding_failed', 'certificate evidence is from a different app or release');
  }
  if (bound.reportId !== expected.reportId) {
    const error = new AttestationError('tls_binding_failed', 'Attestation: certificate evidence is from a different VM than the one that answered');
    throw error;
  }
  const certHash = toHex(await sha256(certPem.body));
  const listed = text(manifest.body).split('\n').some((l) => l === `${certHash}  cert-${url.hostname}.pem`);
  if (!listed) fail('tls_binding_failed', 'certificate is not covered by the attested evidence');

  const leaf = pemCertificates(text(certPem.body))[0];
  if (!leaf) fail('tls_binding_failed', 'evidence certificate is empty');
  // Compare public keys, not whole certificates: a renewal in progress may
  // present a newer certificate for the same attested key. Every connection
  // used above must have presented it.
  const attestedKey = parseCertificate(leaf, 'evidence certificate').spki;
  for (const file of [attestation, manifest, certPem]) {
    if (!file.certificate || !equal(parseCertificate(file.certificate, 'TLS certificate').spki, attestedKey)) {
      fail('tls_binding_failed', 'the TLS certificate on this connection is not the attested one');
    }
  }
  return { status: 'verified', tlsKeySha256: toHex(await sha256(attestedKey)) };
}

/**
 * A `fetch` for Node.js whose every connection must present the attested TLS
 * key (`tlsKeySha256` from a verified binding). The key is checked during the
 * TLS handshake, on each new connection, before any request bytes are sent, so
 * a different TLS endpoint for the same hostname never sees a request. A
 * mismatch rejects with an error whose `code` is `TLS_PIN_MISMATCH`.
 *
 * Responses are buffered. Redirects are returned, not followed: a redirect
 * target would not be the verified origin.
 *
 * @param {string|string[]} tlsKeySha256 - Hex SHA-256 of the attested SubjectPublicKeyInfo, or of each
 *   attested one: during a cutover two verified VMs serve the hostname, each with its own key.
 *   A mismatch error carries the key that was presented as `tlsKeySha256`.
 * @param {{tls?: Object}} [options]
 * @returns {Promise<(input: string|URL, init?: RequestInit) => Promise<Response>>}
 */
export async function createPinnedFetch(tlsKeySha256, { tls: tlsOptions } = {}) {
  const keys = Array.isArray(tlsKeySha256) ? tlsKeySha256 : [tlsKeySha256];
  if (keys.length === 0) fail('invalid_argument', 'tlsKeySha256 is required');
  const pins = new Set(keys.map((key) => hexOfLength(key, 32, 'tlsKeySha256')));
  const [https, tls, { createHash }] = await Promise.all([nodeModule('https'), nodeModule('tls'), nodeModule('crypto')]);
  const agent = new https.Agent({
    keepAlive: true,
    ...(tlsOptions ?? {}),
    // Node only enforces `checkServerIdentity` when it rejects unauthorized
    // peers, so neither the caller's options nor NODE_TLS_REJECT_UNAUTHORIZED
    // may turn that off here: the pin would silently stop applying.
    rejectUnauthorized: true,
    checkServerIdentity(host, cert) {
      const nameError = tls.checkServerIdentity(host, cert);
      if (nameError) return nameError;
      let actual;
      try {
        actual = createHash('sha256').update(parseCertificate(new Uint8Array(cert.raw), 'TLS certificate').spki).digest('hex');
      } catch (e) {
        actual = null;
      }
      if (!pins.has(actual)) {
        const error = new Error('the TLS key on this connection is not the attested one');
        error.code = TLS_PIN_MISMATCH;
        error.tlsKeySha256 = actual;
        return error;
      }
      return undefined;
    },
  });
  return (input, init = {}) =>
    new Promise((resolve, reject) => {
      let url;
      try {
        url = new URL(String(input));
      } catch (e) {
        return reject(new TypeError(`invalid URL: ${input}`));
      }
      if (url.protocol !== 'https:') return reject(new TypeError('a pinned connection needs an https URL'));
      const headers = {};
      new Headers(init.headers ?? {}).forEach((value, key) => {
        headers[key] = value;
      });
      let body = null;
      if (init.body != null) {
        if (typeof init.body === 'string') body = Buffer.from(init.body);
        else if (init.body instanceof Uint8Array) body = Buffer.from(init.body.buffer, init.body.byteOffset, init.body.byteLength);
        else if (init.body instanceof ArrayBuffer) body = Buffer.from(init.body);
        else return reject(new TypeError('a pinned connection supports string and byte bodies'));
        headers['content-length'] = String(body.length);
      }
      const signal = init.signal;
      if (signal?.aborted) return reject(signal.reason ?? new DOMException('This operation was aborted', 'AbortError'));
      const req = https.request(url, { method: init.method ?? 'GET', headers, agent }, (res) => {
        const chunks = [];
        res.on('data', (chunk) => chunks.push(chunk));
        res.on('error', reject);
        res.on('end', () => {
          const responseHeaders = new Headers();
          for (const [key, value] of Object.entries(res.headers)) {
            if (Array.isArray(value)) value.forEach((v) => responseHeaders.append(key, v));
            else if (value !== undefined) responseHeaders.set(key, value);
          }
          const status = res.statusCode ?? 0;
          const noBody = status === 204 || status === 205 || status === 304 || init.method === 'HEAD';
          try {
            resolve(new Response(noBody ? null : Buffer.concat(chunks), { status, statusText: res.statusMessage, headers: responseHeaders }));
          } catch (e) {
            reject(e);
          }
        });
      });
      const onAbort = () => req.destroy(signal.reason ?? new DOMException('This operation was aborted', 'AbortError'));
      signal?.addEventListener('abort', onAbort, { once: true });
      req.on('close', () => signal?.removeEventListener('abort', onAbort));
      req.on('error', reject);
      req.end(body ?? undefined);
    });
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Whole verifications tried before a TLS binding failure is reported.
const BINDING_ATTEMPTS = 3;

/** Resolves the policy for an origin: explicit options over the built-in table. */
export function resolvePolicy(baseUrl, overrides = {}) {
  let origin;
  try {
    origin = new URL(baseUrl).origin;
  } catch (e) {
    fail('invalid_argument', `invalid base URL ${JSON.stringify(baseUrl)}`, e);
  }
  const known = ATTESTED_ORIGINS[origin];
  const policy = { ...(known ?? {}), ...Object.fromEntries(Object.entries(overrides).filter(([, v]) => v !== undefined)) };
  if (!policy.appId || !policy.kmsContract) {
    fail(
      'no_policy',
      LEGACY_ORIGINS[origin]
        ? `${origin} is a legacy origin this SDK does not verify; use ${LEGACY_ORIGINS[origin]}`
        : `no attestation policy is known for ${origin}; pass appId and kmsContract`,
    );
  }
  policy.appId = hexOfLength(policy.appId, 20, 'policy.appId');
  policy.rpcUrls = policy.rpcUrls?.length ? [...policy.rpcUrls] : [...BASE_PUBLIC_RPC_URLS];
  policy.platforms = policy.platforms ?? ['sev-snp'];
  policy.tlsBinding = policy.tlsBinding ?? 'auto';
  if (!TLS_BINDING_MODES.includes(policy.tlsBinding)) {
    fail('invalid_argument', `tlsBinding must be one of ${TLS_BINDING_MODES.join(', ')}, got ${JSON.stringify(policy.tlsBinding)}`);
  }
  // A binding that must hold cannot be satisfied by a connection with no TLS.
  if ((policy.tlsBinding === 'require' || policy.tlsBinding === 'strict') && new URL(origin).protocol !== 'https:') {
    fail('invalid_argument', `tlsBinding ${policy.tlsBinding} needs an https origin, got ${origin}`);
  }
  return { origin, policy };
}

/**
 * @typedef {SnpVerification & {
 *   origin: string,
 *   nonce: string,
 *   verifiedAt: number,
 *   tlsBinding: 'verified'|'unavailable'|'unsupported'|'skipped',
 *   tlsKeySha256: string|null,
 * }} AttestationResult
 */

/**
 * Verifies that `baseUrl` is the governed Lit workload in a genuine
 * confidential VM. Resolves with what was proven; rejects with an
 * `AttestationError` otherwise. Never resolves on a partial check.
 *
 * @param {string} baseUrl - API origin, e.g. `LIT_API_URL`.
 * @param {Partial<AttestationPolicy> & {
 *   timeoutMs?: number,
 *   kdsBaseUrl?: string,
 *   collateral?: {vcek: Uint8Array|string, ask: Uint8Array|string},
 *   fetch?: typeof fetch,
 *   now?: number,
 *   tls?: Object,
 * }} [options] - Policy overrides and transport options.
 * @returns {Promise<AttestationResult>}
 */
export async function verifyAttestation(baseUrl, options = {}) {
  const { timeoutMs = 20_000, kdsBaseUrl, collateral: suppliedCollateral, fetch: fetchOption, now = Date.now(), tls, bindingAttempt = 1, aimTlsKeySha256, ...overrides } = options;
  const fetchImpl = fetchOption ?? ((...args) => globalThis.fetch(...args));
  const { origin, policy } = resolvePolicy(baseUrl, overrides);

  const nonce = globalThis.crypto.getRandomValues(new Uint8Array(32));
  const expectedReportData = attestationV3ReportData(nonce);

  // In Node.js the request is made here, not through `fetch`, so the TLS key
  // of its connection is known: it identifies the VM that answered, which the
  // certificate evidence must then come from.
  let https = null;
  if (!fetchOption && policy.tlsBinding !== 'off' && origin.startsWith('https:')) {
    https = await nodeModule('https').catch(() => null);
  }
  const attestationPath = `/attestation/v3?nonce=${toHex(nonce)}`;
  const request = async () => {
    if (!https) return { res: await fetchImpl(`${origin}${attestationPath}`, { signal: AbortSignal.timeout(timeoutMs), cache: 'no-store' }), key: null };
    const got = await nodeGet(https, new URL(origin), attestationPath, { tls }, Date.now() + timeoutMs, MAX_EVIDENCE_BYTES * 2 + 65536);
    return { res: new Response(got.body, { status: got.status }), key: await connectionKey(got.certificate) };
  };

  // The hardware produces one attestation at a time. The endpoint queues
  // requests for it and answers 429 when the queue is full, which is retried
  // for as long as `timeoutMs`.
  const busyUntil = Date.now() + timeoutMs;
  let body;
  let answeredKey = null;
  let aimed = 0;
  for (let attempt = 0; ; attempt++) {
    let res;
    try {
      ({ res, key: answeredKey } = await request());
    } catch (e) {
      fail('endpoint_unavailable', `could not reach ${origin}/attestation/v3 (${e?.message ?? e})`, e);
    }
    // A caller that was just refused by one VM's key asks for that VM, so the
    // key it learns is the one it needs. Whichever VM answers is verified in
    // full either way.
    if (res.ok && aimTlsKeySha256 && answeredKey && answeredKey !== aimTlsKeySha256 && ++aimed < SAME_VM_ATTEMPTS) {
      attempt--;
      continue;
    }
    if (res.ok) {
      try {
        body = await res.json();
      } catch (e) {
        fail('malformed_evidence', 'attestation response is not JSON', e);
      }
      break;
    }
    if (res.status === 429 || res.status === 503) {
      const wait = Math.min(250 * 2 ** attempt, 2000) + Math.floor(Math.random() * 250);
      if (Date.now() + wait < busyUntil) {
        await sleep(wait);
        continue;
      }
      fail('endpoint_busy', `${origin}/attestation/v3 was still busy (HTTP ${res.status}) after ${timeoutMs} ms of retries`);
    }
    fail('endpoint_unavailable', `${origin}/attestation/v3 answered HTTP ${res.status}`);
  }
  if (body?.version !== 3 || typeof body.attestation !== 'string') fail('malformed_evidence', 'unexpected attestation response');
  if (body.attestation.length > MAX_EVIDENCE_BYTES * 2) fail('malformed_evidence', 'attestation is too large');

  const evidence = decodeAttestation(fromHex(body.attestation, 'attestation'));
  if (!policy.platforms.includes(evidence.platform)) {
    fail('unsupported_platform', `${origin} attests as ${evidence.platform}, which this policy does not accept`);
  }
  if (evidence.platform !== 'sev-snp') fail('unsupported_platform', `${evidence.platform} evidence is not supported by this verifier`);

  // Collateral is AMD-signed, so its source does not matter, and a source that
  // turns out stale or wrong (a cached VCEK from before a firmware update, an
  // envelope chain in another order) must not be worse than having none. Try
  // each in turn: caller, the deployment's own response, the evidence
  // envelope, then AMD directly. Only certificate problems move on to the next
  // source; anything else about the evidence is final.
  const parsed = parseSnpReport(evidence.report);
  const sources = [
    () => normalizeCollateral(suppliedCollateral),
    () => normalizeCollateral(body.collateral),
    () => envelopeCollateral(evidence.certChain),
    () => fetchAmdCollateral(parsed, { kdsBaseUrl, timeoutMs, fetchImpl }),
  ];
  let result;
  let collateral;
  let certificateError;
  for (const source of sources) {
    try {
      collateral = await source();
      if (!collateral) continue;
      result = await verifySnpEvidence(evidence, { expectedReportData, collateral, now });
      break;
    } catch (e) {
      const aboutCertificates = e instanceof AttestationError && ['untrusted_hardware', 'collateral_unavailable', 'malformed_evidence'].includes(e.code);
      if (!aboutCertificates) throw e;
      // Keep the most specific failure: a rejected certificate says more than
      // an unreachable AMD service.
      if (!certificateError || e.code !== 'collateral_unavailable') certificateError = e;
    }
  }
  if (!result) throw certificateError ?? new AttestationError('collateral_unavailable', 'AMD VCEK and ASK certificates are required');
  checkTcbPolicy(result, policy);

  if (result.appId !== policy.appId) fail('identity_mismatch', `${origin} runs app ${result.appId}, not the expected ${policy.appId}`);
  const [composeAllowed, osAllowed] = await Promise.all([
    allowedOnChain(policy.rpcUrls, policy.appId, SELECTOR_ALLOWED_COMPOSE_HASHES, result.composeHash, timeoutMs, fetchImpl),
    allowedOnChain(policy.rpcUrls, policy.kmsContract, SELECTOR_ALLOWED_OS_IMAGES, result.osImageHash, timeoutMs, fetchImpl),
  ]);
  if (!composeAllowed) fail('not_governed', `compose hash ${result.composeHash} is not approved for this app on-chain`);
  if (!osAllowed) fail('not_governed', `OS image ${result.osImageHash} is not approved on-chain`);

  let tlsBinding = 'skipped';
  let tlsKeySha256 = null;
  if (policy.tlsBinding !== 'off') {
    let binding;
    try {
      binding = await verifyTlsBinding(origin, result, { fetchImpl, timeoutMs, kdsBaseUrl, collateral, now, tls, tlsKeySha256: answeredKey });
    } catch (e) {
      // While a deployment cuts over, two VMs serve the hostname and the
      // nonce and the certificate evidence can still come from different ones
      // (always, when the nonce went through `fetch`). Verify again from the
      // start; a deployment whose evidence really does not bind fails each time.
      if (e?.code === 'tls_binding_failed' && bindingAttempt < BINDING_ATTEMPTS) {
        return verifyAttestation(baseUrl, { ...options, bindingAttempt: bindingAttempt + 1 });
      }
      throw e;
    }
    tlsBinding = binding.status;
    tlsKeySha256 = binding.tlsKeySha256 ?? null;
    const mustVerify = policy.tlsBinding === 'strict' || (policy.tlsBinding === 'require' && tlsBinding !== 'unsupported');
    if (tlsBinding !== 'verified' && mustVerify) {
      fail(
        'tls_binding_unavailable',
        tlsBinding === 'unsupported'
          ? 'TLS binding is required but this runtime cannot inspect TLS certificates'
          : 'TLS binding is required but the deployment publishes no certificate evidence',
      );
    }
  }

  return { ...result, origin, nonce: toHex(nonce), verifiedAt: Date.now(), tlsBinding, tlsKeySha256 };
}
