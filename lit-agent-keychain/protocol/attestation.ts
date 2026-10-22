// Remote attestation for the Lit endpoint the SDK sends signed requests to.
//
// Before an agent or owner talks to Chipotle, this module proves the endpoint is
// a genuine Intel TDX machine running the governed Lit Chipotle release:
//
//   1. Parse the TDX v4 quote from GET /attestation and verify its ECDSA-P256
//      signature chain: quote → attestation key → QE report → PCK certificate →
//      Intel SGX PCK CA → pinned Intel SGX Root CA public key.
//   2. Replay the dstack event log and require every RTMR to match the quote.
//   3. Require the event log's app-id to be the expected DstackApp, and its
//      compose-hash to be SHA-256 of the served app_compose with every image
//      digest-pinned.
//   4. Confirm on Base that the compose-hash is whitelisted in DstackApp
//      (governed by the Lit Safe) and the OS image hash is whitelisted in
//      DstackKms (Phala's contract, governed by Phala's Safe).
//   5. Optionally bind the live TLS certificate to the enclave through the
//      dstack-ingress evidence quote (Node only; browsers cannot read peer certs).
//
// That is `verifyAttestation` (v1). SDK connections (Keychain, Owner, the CLI
// and MCP) use it, and it stays exactly as released. It does not bind the
// quote's boot measurements (MRTD, RTMR0-2) to a whitelisted OS image: the
// os-image-hash event it checks is written by the guest, so it cannot tell the
// governed OS from another one on genuine TDX hardware.
//
// `verifyAttestationV2` (the SDK's opt-in `verifyLitEndpoint()`) closes that.
// It runs steps 1-4 against GET /attestation/v2 and step 5 whenever a
// certificate is supplied (`verifyLitEndpoint()` always supplies one in Node;
// browsers cannot, and the report's `channel` says so), and adds, with no
// caller configuration:
//
//   6. Boot binding: verify the dstack KMS signature chain over the endpoint's
//      attestation signer against the KMS root key published on-chain in
//      DstackKms, which must equal the root pinned in this module
//      (`PINNED_KMS_ROOTS`) so a dishonest RPC cannot substitute its own. The
//      KMS releases app keys only to CVMs whose boot measurements (MRTD,
//      RTMR0-2) match a whitelisted OS image and whose compose hash and app id
//      are governed, so a guest-authored event log alone cannot pass.
//   7. KMS identity: the quote's key-provider event must name that same KMS.
//   8. Freshness: the signer signs a random caller nonce together with the hash
//      of the quote it served, so responses cannot be replayed or re-paired.
//      The nonce is always generated here; callers cannot choose it.
//
// The v2 report contains only values taken from authenticated evidence: the
// quote, the event log replayed into its RTMRs, and the app_compose whose hash
// the event log measures. Other /info fields (instance_id, tcb_info's copies of
// the measurements) and vm_config's cpu_count/memory_size are not
// authenticated; they are checked for consistency where listed above and
// otherwise ignored.
//
// Deliberately not covered: Intel TCB status / QE identity collateral and PCK
// revocation. Those need Intel PCS lookups; the dstack-verifier performs them.
// The public /attestation quote carries no caller nonce, so freshness rests on
// the TLS binding (step 5) rather than on the quote itself.
import { p256 } from "@noble/curves/nist.js";
import { secp256k1 } from "@noble/curves/secp256k1.js";
import { sha256, sha384 } from "@noble/hashes/sha2.js";
import { keccak_256 } from "@noble/hashes/sha3.js";
import { hex, unhex, requireThat } from "./crypto.ts";
import { randomBytes } from "./crypto.ts";
import { jsonFetch, textFetch } from "./http.ts";
// The governance lookup talks to public chain RPCs, which rate limit and go
// down independently of Lit. It uses the client fetch so a 429 or 5xx can be
// named and the next endpoint tried; the template helper above stays untouched.
import { jsonFetch as rpcFetch, HttpError } from "./client-http.ts";
// The v2 endpoint fetch uses it too, so a missing endpoint (404) is told from a
// broken one by its status.
import { jsonFetch as clientJsonFetch } from "./client-http.ts";

export type AttestationPolicy = {
  /** Expected dstack app id: the DstackApp contract address, hex without 0x. */
  appId: string;
  /** DstackKms contract that whitelists OS images. */
  kmsContract: string;
  /** JSON-RPC endpoint for the chain that hosts both contracts; tried first. */
  rpcUrl: string;
  /**
   * Tried in order when `rpcUrl` is unreachable, rate limited or returns an RPC
   * error. The first endpoint that answers decides; a "not whitelisted" answer
   * is final and never retried elsewhere.
   */
  fallbackRpcUrls?: string[];
};
/**
 * Public Base mainnet endpoints. Each is free and independently operated, and
 * each throttles bursts from one IP (mainnet.base.org after roughly three CLI
 * runs in quick succession), so the governance check rotates through them.
 * Pin your own endpoint with `attestation.rpcUrl` or `KEYCHAIN_BASE_RPC_URL`
 * for stronger guarantees than a public RPC offers.
 */
export const BASE_PUBLIC_RPC_URLS = [
  "https://mainnet.base.org",
  "https://base-rpc.publicnode.com",
  "https://1rpc.io/base",
  "https://base.drpc.org",
];
export const CHIPOTLE_ATTESTATION_POLICY: AttestationPolicy = {
  appId: "3f91deaf16ff7c823ee65081d6bafa1ceea05ffc",
  kmsContract: "0x2f83172a49584c017f2b256f0fb2dca14126ba9c",
  rpcUrl: BASE_PUBLIC_RPC_URLS[0],
  fallbackRpcUrls: BASE_PUBLIC_RPC_URLS.slice(1),
};
export const ATTESTED_ORIGINS: Record<string, AttestationPolicy> = {
  "https://api.chipotle.litprotocol.com": CHIPOTLE_ATTESTATION_POLICY,
};

// Intel SGX Root CA (certificates.trustedservices.intel.com), uncompressed P-256.
const INTEL_SGX_ROOT_PUBLIC_KEY =
  "040ba9c4c0c0c86193a3fe23d6b02cda10a8bbd4e88e48b4458561a36e705525f567918e2edc88e40d860bd0cc4ee26aacc988e505a953558c453f6b0904ae7394";
const OID_ECDSA_WITH_SHA256 = "2a8648ce3d040302";
const OID_EC_PUBLIC_KEY = "2a8648ce3d0201";
const OID_PRIME256V1 = "2a8648ce3d030107";
const OID_COMMON_NAME = "550403";
const DSTACK_EVENT = 0x08000001;
const SELECTOR_ALLOWED_COMPOSE_HASHES = "2f6622e5";
const SELECTOR_ALLOWED_OS_IMAGES = "9a4e1d18";
const SELECTOR_KMS_INFO = "09177063";
const TRUE_WORD = "0x" + "0".repeat(63) + "1";

const le16 = (b: Uint8Array, o: number) => b[o] | (b[o + 1] << 8);
const le32 = (b: Uint8Array, o: number) =>
  (b[o] | (b[o + 1] << 8) | (b[o + 2] << 16) | (b[o + 3] << 24)) >>> 0;
const ascii = (s: string) => new TextEncoder().encode(s);
const concat = (...parts: Uint8Array[]) => {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
};
const equal = (a: Uint8Array, b: Uint8Array) =>
  a.length === b.length && a.every((v, i) => v === b[i]);
const strip0x = (s: string) => s.trim().toLowerCase().replace(/^0x/, "");
const hexBytes = (s: string, bytes: number, what: string) => {
  const clean = strip0x(s);
  requireThat(
    new RegExp(`^[0-9a-f]{${bytes * 2}}$`).test(clean),
    `Attestation: ${what} must be ${bytes} bytes of hex`,
  );
  return clean;
};

// ---- minimal DER ------------------------------------------------------------
type Tlv = { tag: number; start: number; end: number; headerStart: number };
function tlv(b: Uint8Array, pos: number, limit = b.length): Tlv {
  requireThat(pos + 2 <= limit, "Attestation: truncated DER");
  const tag = b[pos];
  let len = b[pos + 1];
  let p = pos + 2;
  if (len & 0x80) {
    const n = len & 0x7f;
    requireThat(
      n >= 1 && n <= 4 && p + n <= limit,
      "Attestation: bad DER length",
    );
    len = 0;
    for (let i = 0; i < n; i++) len = len * 256 + b[p + i];
    p += n;
  }
  requireThat(p + len <= limit, "Attestation: DER overruns buffer");
  return { tag, start: p, end: p + len, headerStart: pos };
}
function children(b: Uint8Array, node: Tlv): Tlv[] {
  const out: Tlv[] = [];
  let p = node.start;
  while (p < node.end) {
    const c = tlv(b, p, node.end);
    out.push(c);
    p = c.end;
  }
  return out;
}
const slice = (b: Uint8Array, n: Tlv) => b.subarray(n.start, n.end);
const whole = (b: Uint8Array, n: Tlv) => b.subarray(n.headerStart, n.end);
/** DER ECDSA SEQUENCE { INTEGER r, INTEGER s } → 64-byte compact signature. */
function compactSignature(der: Uint8Array): Uint8Array {
  const seq = tlv(der, 0);
  requireThat(
    seq.tag === 0x30 && seq.end === der.length,
    "Attestation: bad ECDSA DER",
  );
  const [r, s] = children(der, seq);
  requireThat(r?.tag === 0x02 && s?.tag === 0x02, "Attestation: bad ECDSA DER");
  const int = (n: Tlv) => {
    let v = slice(der, n);
    while (v.length > 32 && v[0] === 0) v = v.subarray(1);
    requireThat(v.length <= 32, "Attestation: ECDSA integer too long");
    const out = new Uint8Array(32);
    out.set(v, 32 - v.length);
    return out;
  };
  return concat(int(r), int(s));
}
function parseTime(node: Tlv, b: Uint8Array): number {
  const t = new TextDecoder().decode(slice(b, node));
  const m =
    node.tag === 0x17
      ? /^(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})Z$/.exec(t)
      : node.tag === 0x18
        ? /^(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})Z$/.exec(t)
        : null;
  requireThat(m, "Attestation: bad certificate time");
  const year =
    node.tag === 0x17
      ? Number(m[1]) + (Number(m[1]) >= 50 ? 1900 : 2000)
      : Number(m[1]);
  return Date.UTC(
    year,
    Number(m[2]) - 1,
    Number(m[3]),
    Number(m[4]),
    Number(m[5]),
    Number(m[6]),
  );
}
export type Certificate = {
  tbs: Uint8Array;
  signature: Uint8Array;
  publicKey: Uint8Array;
  commonName: string;
  notBefore: number;
  notAfter: number;
};
export function parseCertificate(der: Uint8Array): Certificate {
  const cert = tlv(der, 0);
  requireThat(
    cert.tag === 0x30 && cert.end === der.length,
    "Attestation: bad certificate",
  );
  const [tbs, sigAlg, sigValue] = children(der, cert);
  requireThat(
    tbs?.tag === 0x30 && sigAlg?.tag === 0x30 && sigValue?.tag === 0x03,
  );
  const algOid = children(der, sigAlg)[0];
  requireThat(
    algOid?.tag === 0x06 && hex(slice(der, algOid)) === OID_ECDSA_WITH_SHA256,
    "Attestation: certificate is not ECDSA-with-SHA256",
  );
  const fields = children(der, tbs);
  if (fields[0]?.tag === 0xa0) fields.shift(); // explicit version
  const [, , , validity, subject, spki] = fields;
  requireThat(
    validity?.tag === 0x30 && subject?.tag === 0x30 && spki?.tag === 0x30,
  );
  const [nb, na] = children(der, validity);
  const [spkiAlg, spkiKey] = children(der, spki);
  const [keyOid, curveOid] = children(der, spkiAlg);
  requireThat(
    hex(slice(der, keyOid)) === OID_EC_PUBLIC_KEY &&
      hex(slice(der, curveOid)) === OID_PRIME256V1 &&
      spkiKey.tag === 0x03,
    "Attestation: certificate key is not P-256",
  );
  const publicKey = slice(der, spkiKey).subarray(1);
  requireThat(
    publicKey.length === 65 && publicKey[0] === 4,
    "Attestation: bad P-256 key",
  );
  let commonName = "";
  for (const rdn of children(der, subject))
    for (const atv of children(der, rdn)) {
      const [oid, value] = children(der, atv);
      if (hex(slice(der, oid)) === OID_COMMON_NAME)
        commonName = new TextDecoder().decode(slice(der, value));
    }
  return {
    tbs: whole(der, tbs),
    signature: compactSignature(slice(der, sigValue).subarray(1)),
    publicKey,
    commonName,
    notBefore: parseTime(nb, der),
    notAfter: parseTime(na, der),
  };
}
export function pemCertificates(pem: string): Uint8Array[] {
  const out: Uint8Array[] = [];
  const re =
    /-----BEGIN CERTIFICATE-----([A-Za-z0-9+/=\s]+?)-----END CERTIFICATE-----/g;
  for (const m of pem.matchAll(re))
    out.push(
      Uint8Array.from(atob(m[1].replace(/\s+/g, "")), (c) => c.charCodeAt(0)),
    );
  return out;
}
const verifyP256 = (sig: Uint8Array, message: Uint8Array, key: Uint8Array) =>
  p256.verify(sig, sha256(message), key, { prehash: false, lowS: false });

// ---- TDX quote ---------------------------------------------------------------
export type TdxQuote = {
  measurements: {
    mrtd: string;
    rtmr0: string;
    rtmr1: string;
    rtmr2: string;
    rtmr3: string;
  };
  teeTcbSvn: string;
  tdAttributes: string;
  reportData: string;
  pckNotAfter: number;
};
/**
 * Parses a TDX v4 ECDSA-P256 quote and verifies its complete signature chain to
 * the pinned Intel SGX Root CA. Throws on any structural or cryptographic fault.
 */
export function verifyQuote(quoteHex: string, now = Date.now()): TdxQuote {
  const b = unhex(strip0x(quoteHex));
  requireThat(b.length >= 636 + 4, "Attestation: quote too short");
  requireThat(le16(b, 0) === 4, "Attestation: unsupported quote version");
  requireThat(le16(b, 2) === 2, "Attestation: quote is not ECDSA-P256");
  requireThat(le32(b, 4) === 0x81, "Attestation: quote is not from TDX");
  const body = b.subarray(48, 632);
  const tdAttributes = body.subarray(120, 128);
  requireThat(
    (tdAttributes[0] & 1) === 0,
    "Attestation: TD runs in debug mode",
  );
  const signedLength = le32(b, 632);
  // dstack may append trailing bytes after the signature data; they are unsigned
  // and ignored, exactly as dcap-qvl does.
  requireThat(
    signedLength >= 134 && 636 + signedLength <= b.length,
    "Attestation: quote length mismatch",
  );
  const s = b.subarray(636, 636 + signedLength);
  const signature = s.subarray(0, 64);
  const attestationKey = concat(new Uint8Array([4]), s.subarray(64, 128));
  requireThat(
    le16(s, 128) === 6,
    "Attestation: unsupported certification data",
  );
  const certData = s.subarray(134, 134 + le32(s, 130));
  requireThat(
    certData.length >= 450,
    "Attestation: truncated QE certification data",
  );
  const qeReport = certData.subarray(0, 384);
  const qeSignature = certData.subarray(384, 448);
  const authLength = le16(certData, 448);
  const qeAuth = certData.subarray(450, 450 + authLength);
  const inner = 450 + authLength;
  requireThat(le16(certData, inner) === 5, "Attestation: PCK chain is not PEM");
  const pem = new TextDecoder().decode(
    certData.subarray(inner + 6, inner + 6 + le32(certData, inner + 2)),
  );
  // 1. Quote header+body signed by the attestation key.
  requireThat(
    verifyP256(signature, b.subarray(0, 632), attestationKey),
    "Attestation: quote signature invalid",
  );
  // 2. QE report binds the attestation key.
  requireThat(
    equal(
      qeReport.subarray(320, 352),
      sha256(concat(s.subarray(64, 128), qeAuth)),
    ),
    "Attestation: QE report does not bind the attestation key",
  );
  // 3. PCK chain to the pinned Intel root, then QE report signed by the PCK leaf.
  const chain = pemCertificates(pem).map(parseCertificate);
  requireThat(
    chain.length === 3,
    "Attestation: PCK chain must have three certificates",
  );
  const [leaf, ca, root] = chain;
  requireThat(
    hex(root.publicKey) === INTEL_SGX_ROOT_PUBLIC_KEY &&
      root.commonName === "Intel SGX Root CA",
    "Attestation: PCK chain does not end at the Intel SGX Root CA",
  );
  requireThat(
    /^Intel SGX PCK (Platform|Processor) CA$/.test(ca.commonName) &&
      leaf.commonName === "Intel SGX PCK Certificate",
    "Attestation: unexpected PCK certificate names",
  );
  for (const c of chain)
    requireThat(
      now >= c.notBefore && now <= c.notAfter,
      "Attestation: PCK certificate expired",
    );
  requireThat(
    verifyP256(root.signature, root.tbs, root.publicKey),
    "Attestation: root self-signature invalid",
  );
  requireThat(
    verifyP256(ca.signature, ca.tbs, root.publicKey),
    "Attestation: PCK CA signature invalid",
  );
  requireThat(
    verifyP256(leaf.signature, leaf.tbs, ca.publicKey),
    "Attestation: PCK leaf signature invalid",
  );
  requireThat(
    verifyP256(qeSignature, qeReport, leaf.publicKey),
    "Attestation: QE report signature invalid",
  );
  const m = (o: number) => hex(body.subarray(o, o + 48));
  return {
    measurements: {
      mrtd: m(136),
      rtmr0: m(328),
      rtmr1: m(376),
      rtmr2: m(424),
      rtmr3: m(472),
    },
    teeTcbSvn: hex(body.subarray(0, 16)),
    tdAttributes: hex(tdAttributes),
    reportData: hex(body.subarray(520, 584)),
    pckNotAfter: leaf.notAfter,
  };
}

// ---- dstack event log --------------------------------------------------------
export type EventLogReplay = {
  rtmrs: [string, string, string, string];
  events: Record<string, string>;
};
/** Replays the dstack event log into RTMR0-3 and collects dstack runtime events. */
export function replayEventLog(eventLog: string): EventLogReplay {
  const entries = JSON.parse(eventLog);
  requireThat(
    Array.isArray(entries) && entries.length <= 4096,
    "Attestation: bad event log",
  );
  const rtmrs: Uint8Array[] = [0, 1, 2, 3].map(() => new Uint8Array(48));
  const events: Record<string, string> = {};
  const seen = new Set<string>();
  for (const e of entries) {
    requireThat(
      Number.isInteger(e?.imr) &&
        e.imr >= 0 &&
        e.imr <= 3 &&
        Number.isInteger(e.event_type),
      "Attestation: bad event log entry",
    );
    let digest: Uint8Array;
    if (e.event_type === DSTACK_EVENT) {
      requireThat(
        typeof e.event === "string" &&
          /^([0-9a-f]{2})*$/.test(e.event_payload ?? ""),
      );
      const typeLe = new Uint8Array([1, 0, 0, 8]);
      digest = sha384(
        concat(
          typeLe,
          ascii(":"),
          ascii(e.event),
          ascii(":"),
          e.event_payload ? unhex(e.event_payload) : new Uint8Array(),
        ),
      );
      if (e.digest)
        requireThat(
          hex(digest) === e.digest,
          `Attestation: event ${e.event} digest mismatch`,
        );
      requireThat(e.imr === 3, "Attestation: dstack event outside RTMR3");
      requireThat(
        !seen.has(e.event) || e.event_payload === "",
        `Attestation: duplicate event ${e.event}`,
      );
      seen.add(e.event);
      if (e.event_payload) events[e.event] = e.event_payload;
    } else {
      requireThat(
        /^[0-9a-f]{96}$/.test(e.digest ?? ""),
        "Attestation: bad event digest",
      );
      digest = unhex(e.digest);
    }
    rtmrs[e.imr] = sha384(concat(rtmrs[e.imr], digest));
  }
  return { rtmrs: rtmrs.map(hex) as EventLogReplay["rtmrs"], events };
}

// ---- compose / governance ----------------------------------------------------
export function pinnedImages(appCompose: string): string[] {
  const parsed = JSON.parse(appCompose);
  const composeFile = parsed?.docker_compose_file;
  requireThat(
    typeof composeFile === "string",
    "Attestation: app_compose lacks docker_compose_file",
  );
  const images = [
    ...composeFile.matchAll(/^\s*image:\s*["']?([^\s"'#]+)/gm),
  ].map((m) => m[1]);
  requireThat(
    images.length > 0,
    "Attestation: compose file declares no images",
  );
  for (const image of images)
    requireThat(
      /@sha256:[0-9a-f]{64}$/.test(image),
      `Attestation: image not digest-pinned: ${image}`,
    );
  return images;
}
/** Short, human-readable reason one RPC endpoint could not answer. */
function describeRpcFailure(error: unknown) {
  if (error instanceof HttpError)
    return error.status === 429
      ? "429 rate limited"
      : `HTTP ${error.status}${error.detail ? ` ${error.detail}` : ""}`;
  const message = error instanceof Error ? error.message : String(error);
  return message.replace(/^Request failed \((\d+)\)$/, "HTTP $1");
}
/**
 * Asks the chain whether `hash32` is whitelisted in `contract`, trying each RPC
 * endpoint in turn until one answers. Only transport, HTTP and JSON-RPC-level
 * failures fall through; a successful `eth_call` result is authoritative.
 */
async function allowedOnChain(
  rpcUrls: string[],
  contract: string,
  selector: string,
  hash32: string,
  timeoutMs: number,
) {
  const to = "0x" + hexBytes(contract, 20, "contract address");
  const body = JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    method: "eth_call",
    params: [{ to, data: "0x" + selector + hash32 }, "latest"],
  });
  const failures: string[] = [];
  for (const rpcUrl of rpcUrls) {
    let result: any;
    try {
      result = await rpcFetch(
        rpcUrl,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body,
        },
        timeoutMs,
      );
    } catch (error) {
      failures.push(`${new URL(rpcUrl).host}: ${describeRpcFailure(error)}`);
      continue;
    }
    if (result?.error !== undefined) {
      failures.push(
        `${new URL(rpcUrl).host}: RPC error${
          typeof result.error?.message === "string"
            ? ` ${result.error.message.slice(0, 120)}`
            : ""
        }`,
      );
      continue;
    }
    return result?.result === TRUE_WORD;
  }
  throw new Error(
    `Attestation: no Base RPC endpoint answered the on-chain governance check (${failures.join(
      "; ",
    )}). Retry shortly, or point the check at your own Base RPC with KEYCHAIN_BASE_RPC_URL (CLI, MCP) or the attestation.rpcUrl option (SDK).`,
  );
}
/**
 * Runs one `eth_call`, trying each RPC endpoint in turn until one answers. Only
 * transport, HTTP and JSON-RPC-level failures fall through; the first result is
 * authoritative.
 */
async function ethCall(
  rpcUrls: string[],
  contract: string,
  data: string,
  timeoutMs: number,
): Promise<unknown> {
  const to = "0x" + hexBytes(contract, 20, "contract address");
  const body = JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    method: "eth_call",
    params: [{ to, data: "0x" + data }, "latest"],
  });
  const failures: string[] = [];
  for (const rpcUrl of rpcUrls) {
    let result: any;
    try {
      result = await rpcFetch(
        rpcUrl,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body,
        },
        timeoutMs,
      );
    } catch (error) {
      failures.push(`${new URL(rpcUrl).host}: ${describeRpcFailure(error)}`);
      continue;
    }
    if (result?.error !== undefined) {
      failures.push(
        `${new URL(rpcUrl).host}: RPC error${
          typeof result.error?.message === "string"
            ? ` ${result.error.message.slice(0, 120)}`
            : ""
        }`,
      );
      continue;
    }
    return result?.result;
  }
  throw new Error(
    `Attestation: no Base RPC endpoint answered the on-chain governance check (${failures.join(
      "; ",
    )}). Retry shortly, or point the check at your own Base RPC with KEYCHAIN_BASE_RPC_URL (CLI, MCP) or the attestation.rpcUrl option (SDK).`,
  );
}
/** The KMS keys DstackKms publishes: secp256k1 root key and P-256 CA key (DER SPKI). */
export type KmsInfo = { k256PublicKey: string; caPublicKey: string };
/** Decodes the first two `bytes` members of the `kmsInfo()` getter's return data. */
export function decodeKmsInfo(result: unknown): KmsInfo {
  requireThat(
    typeof result === "string" && /^0x([0-9a-f]{2})+$/i.test(result),
    "Attestation: malformed kmsInfo() result",
  );
  const data = unhex(result.slice(2).toLowerCase());
  const word = (offset: number) => {
    requireThat(
      offset >= 0 && offset + 32 <= data.length,
      "Attestation: malformed kmsInfo() result",
    );
    const head = data.subarray(offset, offset + 24);
    requireThat(
      head.every((b) => b === 0),
      "Attestation: malformed kmsInfo() result",
    );
    return Number(BigInt("0x" + hex(data.subarray(offset + 24, offset + 32))));
  };
  const bytesAt = (index: number) => {
    const offset = word(index * 32);
    const length = word(offset);
    requireThat(
      offset + 32 + length <= data.length,
      "Attestation: malformed kmsInfo() result",
    );
    return hex(data.subarray(offset + 32, offset + 32 + length));
  };
  return { k256PublicKey: bytesAt(0), caPublicKey: bytesAt(1) };
}
/**
 * Root keys of the dstack KMS deployments this SDK knows, by DstackKms contract
 * (lowercase hex, no 0x). Phala's KMS on Base: read from `kmsInfo()` at Base
 * block 52824745 (2026-10-19) on mainnet.base.org, and identical on
 * base-rpc.publicnode.com and base.drpc.org.
 *
 * `verifyAttestationV2` requires the keys it reads on-chain to equal these, so a
 * dishonest Base RPC cannot substitute a KMS root of its own and with it the
 * boot binding. A KMS rotation therefore needs an SDK release; it fails closed
 * (`KMS_MISMATCH`) until then.
 */
export const PINNED_KMS_ROOTS: Readonly<Record<string, Readonly<KmsInfo>>> =
  Object.freeze({
    "2f83172a49584c017f2b256f0fb2dca14126ba9c": Object.freeze({
      k256PublicKey:
        "02a335aea3df11d46ae0a373d3f26fbd1d208572ecc87e630e618dac693b7af721",
      caPublicKey:
        "3059301306072a8648ce3d020106082a8648ce3d030107034200048bf84f52911056474c207cb50b4e8010d984f8975ea57b8f0c8b17502181c283e93a68a9864ad19f629508cc699f125be3871ec2e26ab0597e5bca4e29b7425f",
    }),
  });

// ---- full verification -------------------------------------------------------
export type AttestationReport = {
  origin: string;
  appId: string;
  composeHash: string;
  osImageHash: string;
  instanceId: string;
  mrKms: string;
  measurements: TdxQuote["measurements"];
  teeTcbSvn: string;
  images: string[];
  checks: string[];
  tlsCertificateSha256?: string;
  verifiedAt: number;
};
export type VerifyOptions = {
  now?: number;
  timeoutMs?: number;
  /** SHA-256 of the DER certificate the caller observed on the TLS connection. */
  tlsCertificateSha256?: string;
};
/**
 * Verifies that `origin` is a genuine, governed Lit TEE deployment. Fails closed:
 * any unmet check throws with an `Attestation:` prefixed message.
 *
 * The original (v1) check, kept exactly as released; SDK connections use it.
 * It does not bind the quote's boot measurements to the approved OS image and
 * has no freshness. `verifyAttestationV2` (`verifyLitEndpoint()` in the SDK)
 * adds both, for callers that opt in.
 */
export async function verifyAttestation(
  origin: string,
  policy: AttestationPolicy,
  options: VerifyOptions = {},
): Promise<AttestationReport> {
  const now = options.now ?? Date.now();
  const timeoutMs = options.timeoutMs ?? 30000;
  const appId = hexBytes(policy.appId, 20, "policy appId");
  const checks: string[] = [];
  const [attestation, info] = await Promise.all([
    jsonFetch(`${origin}/attestation`, {}, timeoutMs),
    jsonFetch(`${origin}/info`, {}, timeoutMs),
  ]);
  requireThat(
    typeof attestation?.quote === "string" &&
      typeof attestation.event_log === "string" &&
      typeof attestation.vm_config === "string",
    "Attestation: malformed /attestation response",
  );
  const quote = verifyQuote(attestation.quote, now);
  checks.push("tdx-quote-signature-chain");
  const replay = replayEventLog(attestation.event_log);
  const { measurements } = quote;
  requireThat(
    replay.rtmrs[0] === measurements.rtmr0 &&
      replay.rtmrs[1] === measurements.rtmr1 &&
      replay.rtmrs[2] === measurements.rtmr2 &&
      replay.rtmrs[3] === measurements.rtmr3,
    "Attestation: event log does not reproduce the quoted RTMRs",
  );
  checks.push("event-log-replay");
  const events = replay.events;
  requireThat(
    events["app-id"] === appId,
    "Attestation: endpoint runs a different dstack app",
  );
  const composeHash = hexBytes(
    events["compose-hash"] ?? "",
    32,
    "compose-hash event",
  );
  const osImageHash = hexBytes(
    events["os-image-hash"] ?? "",
    32,
    "os-image-hash event",
  );
  const tcb =
    typeof info?.tcb_info === "string"
      ? JSON.parse(info.tcb_info)
      : info?.tcb_info;
  requireThat(
    typeof tcb?.app_compose === "string",
    "Attestation: /info lacks app_compose",
  );
  requireThat(
    hex(sha256(ascii(tcb.app_compose))) === composeHash,
    "Attestation: served app_compose does not hash to the measured compose-hash",
  );
  requireThat(
    strip0x(info.app_id ?? "") === appId &&
      strip0x(info.compose_hash ?? "") === composeHash,
    "Attestation: /info disagrees with the measured identity",
  );
  const vmConfig = JSON.parse(attestation.vm_config);
  requireThat(
    strip0x(vmConfig?.os_image_hash ?? "") === osImageHash,
    "Attestation: vm_config OS image differs from the measured one",
  );
  checks.push("measured-identity");
  const images = pinnedImages(tcb.app_compose);
  checks.push("images-digest-pinned");
  const rpcUrls = [policy.rpcUrl, ...(policy.fallbackRpcUrls ?? [])];
  const [composeAllowed, osAllowed] = await Promise.all([
    allowedOnChain(
      rpcUrls,
      appId,
      SELECTOR_ALLOWED_COMPOSE_HASHES,
      composeHash,
      timeoutMs,
    ),
    allowedOnChain(
      rpcUrls,
      policy.kmsContract,
      SELECTOR_ALLOWED_OS_IMAGES,
      osImageHash,
      timeoutMs,
    ),
  ]);
  requireThat(
    composeAllowed,
    "Attestation: compose-hash is not whitelisted in DstackApp",
  );
  requireThat(
    osAllowed,
    "Attestation: OS image is not whitelisted in DstackKms",
  );
  checks.push("onchain-governance");
  const report: AttestationReport = {
    origin,
    appId,
    composeHash,
    osImageHash,
    instanceId: events["instance-id"] ?? "",
    mrKms: events["mr-kms"] ?? "",
    measurements,
    teeTcbSvn: quote.teeTcbSvn,
    images,
    checks,
    verifiedAt: now,
  };
  if (options.tlsCertificateSha256) {
    await verifyTlsBinding(
      origin,
      measurements,
      options.tlsCertificateSha256,
      now,
      timeoutMs,
    );
    report.tlsCertificateSha256 = strip0x(options.tlsCertificateSha256);
    checks.push("tls-certificate-in-tee");
  }
  return report;
}

// ---- v2: KMS-certified boot binding and freshness -----------------------------
type MeasuredIdentity = {
  events: Record<string, string>;
  composeHash: string;
  osImageHash: string;
  images: string[];
};
/**
 * Steps 2-3 for v2: replay the event log against an authenticated quote and
 * check the measured app identity against the served metadata. The same checks
 * `verifyAttestation` runs inline; v1 keeps its own copy so it stays exactly as
 * released.
 */
function measuredIdentity(
  appId: string,
  quote: TdxQuote,
  attestation: any,
  info: any,
  checks: string[],
): MeasuredIdentity {
  const replay = replayEventLog(attestation.event_log);
  const { measurements } = quote;
  requireThat(
    replay.rtmrs[0] === measurements.rtmr0 &&
      replay.rtmrs[1] === measurements.rtmr1 &&
      replay.rtmrs[2] === measurements.rtmr2 &&
      replay.rtmrs[3] === measurements.rtmr3,
    "Attestation: event log does not reproduce the quoted RTMRs",
  );
  checks.push("event-log-replay");
  const events = replay.events;
  requireThat(
    events["app-id"] === appId,
    "Attestation: endpoint runs a different dstack app",
  );
  const composeHash = hexBytes(
    events["compose-hash"] ?? "",
    32,
    "compose-hash event",
  );
  const osImageHash = hexBytes(
    events["os-image-hash"] ?? "",
    32,
    "os-image-hash event",
  );
  const tcb =
    typeof info?.tcb_info === "string"
      ? JSON.parse(info.tcb_info)
      : info?.tcb_info;
  requireThat(
    typeof tcb?.app_compose === "string",
    "Attestation: /info lacks app_compose",
  );
  requireThat(
    hex(sha256(ascii(tcb.app_compose))) === composeHash,
    "Attestation: served app_compose does not hash to the measured compose-hash",
  );
  requireThat(
    strip0x(info.app_id ?? "") === appId &&
      strip0x(info.compose_hash ?? "") === composeHash,
    "Attestation: /info disagrees with the measured identity",
  );
  const vmConfig = JSON.parse(attestation.vm_config);
  requireThat(
    strip0x(vmConfig?.os_image_hash ?? "") === osImageHash,
    "Attestation: vm_config OS image differs from the measured one",
  );
  checks.push("measured-identity");
  const images = pinnedImages(tcb.app_compose);
  checks.push("images-digest-pinned");
  return { events, composeHash, osImageHash, images };
}
/** dstack key purpose of the endpoint's attestation signer (lit-api-server). */
export const ATTESTATION_V2_SIGNER_PURPOSE = "chipotle-attestation-v2";
const STATEMENT_DOMAIN = ascii("chipotle-attestation-v2:");
const KMS_ISSUED_PREFIX = ascii("dstack-kms-issued:");

/** Stable reasons `verifyLitEndpoint` / `verifyAttestationV2` reject an endpoint. */
export type AttestationErrorCode =
  /** The origin has no built-in attestation policy. */
  | "UNKNOWN_ORIGIN"
  /** The attestation policy itself is malformed (app id, KMS contract, RPC). */
  | "INVALID_POLICY"
  /** The endpoint does not serve /attestation/v2 yet (HTTP 404 or 405). */
  | "ENDPOINT_TOO_OLD"
  /** The endpoint could not be reached or returned an error. */
  | "ENDPOINT_UNREACHABLE"
  /** The endpoint returned a response that is not a v2 attestation. */
  | "MALFORMED_RESPONSE"
  /** The TDX quote's Intel signature chain or structure is invalid. */
  | "QUOTE_INVALID"
  /** The event log, app id, compose or OS image metadata are inconsistent. */
  | "MEASURED_IDENTITY_MISMATCH"
  /** The KMS signature chain does not certify the endpoint's signer. */
  | "BOOT_BINDING_FAILED"
  /** The quote names a different KMS than the governed one on-chain. */
  | "KMS_MISMATCH"
  /** The response does not answer this request's nonce and quote. */
  | "STALE_OR_REPLAYED"
  /** The compose hash or OS image is not whitelisted on-chain. */
  | "NOT_GOVERNED"
  /** No Base RPC endpoint answered the governance lookups. */
  | "GOVERNANCE_UNAVAILABLE"
  /** The TLS certificate in use is not the one the enclave attested. */
  | "TLS_BINDING_FAILED";
export class AttestationError extends Error {
  readonly code: AttestationErrorCode;
  constructor(code: AttestationErrorCode, message: string) {
    super(
      message.startsWith("Attestation:") ? message : `Attestation: ${message}`,
    );
    this.name = "AttestationError";
    this.code = code;
  }
}
function fail(code: AttestationErrorCode, message: string): never {
  throw new AttestationError(code, message);
}
/** Runs `fn`, converting any non-AttestationError failure into `code`. */
async function step<T>(
  code: AttestationErrorCode,
  fn: () => T | Promise<T>,
): Promise<T> {
  try {
    return await fn();
  } catch (error) {
    if (error instanceof AttestationError) throw error;
    throw new AttestationError(
      code,
      error instanceof Error ? error.message : String(error),
    );
  }
}

/** What `verifyAttestationV2` proved, on top of the v1 report fields. */
export type AttestationReportV2 = AttestationReport & {
  version: 2;
  /** The quote's boot measurements are certified by the governed dstack KMS. */
  bootBinding: "kms-certified";
  /** The response signed this request's random nonce. */
  freshness: { nonce: string };
  kms: { contract: string; k256PublicKey: string; caPublicKey: string };
  /** Compressed secp256k1 key of the endpoint's KMS-certified signer. */
  signer: string;
  /**
   * `tls-certificate-bound`: the TLS certificate this origin served during
   * verification (`tlsCertificateSha256`) is one the attested enclave holds.
   * That proves nothing about other connections: requests are bound to the
   * attested enclave only if the connection carrying them pins this
   * certificate. SDK connections (Keychain, Owner, CLI, MCP) do not pin it.
   * `not-bound`: the runtime cannot read peer certificates (browsers). The
   * result proves genuine Lit evidence was served at this origin, but a party
   * able to impersonate the origin's TLS could relay that evidence and answer
   * later requests itself; only the browser's own TLS validation protects them.
   */
  channel: "tls-certificate-bound" | "not-bound";
};
/** keccak256 digest the endpoint signs for one response. */
export function attestationStatementDigest(
  nonce: Uint8Array,
  quote: Uint8Array,
): Uint8Array {
  return keccak_256(concat(STATEMENT_DOMAIN, nonce, keccak_256(quote)));
}
/** Parses dstack's 65-byte r || s || v recoverable signature (hex). */
function recoverableSignature(sigHex: unknown, what: string) {
  requireThat(
    typeof sigHex === "string" && /^[0-9a-f]{130}$/.test(sigHex),
    `Attestation: ${what} must be a 65-byte recoverable signature`,
  );
  const bytes = unhex(sigHex);
  const v = bytes[64] >= 27 ? bytes[64] - 27 : bytes[64];
  requireThat(v === 0 || v === 1, `Attestation: ${what} has a bad recovery id`);
  return secp256k1.Signature.fromBytes(
    bytes.subarray(0, 64),
    "compact",
  ).addRecoveryBit(v);
}
/** Compressed public key that produced `sigHex` over `digest`. */
function recoverSigner(sigHex: unknown, digest: Uint8Array, what: string) {
  return hex(
    recoverableSignature(sigHex, what).recoverPublicKey(digest).toBytes(true),
  );
}
/**
 * Verifies dstack's KMS signature chain for the endpoint signer:
 * the app key signed `purpose:signer`, and the KMS root key signed
 * `dstack-kms-issued:` || app id || app key. Returns nothing; throws on failure.
 */
export function verifyKmsSignatureChain(
  appId: string,
  signer: {
    public_key: unknown;
    purpose: unknown;
    app_signature: unknown;
    kms_signature: unknown;
  },
  kmsK256PublicKey: string,
) {
  requireThat(
    signer.purpose === ATTESTATION_V2_SIGNER_PURPOSE,
    "Attestation: endpoint signer has an unexpected purpose",
  );
  const signerKey = signer.public_key;
  requireThat(
    typeof signerKey === "string" && /^0[23][0-9a-f]{64}$/.test(signerKey),
    "Attestation: endpoint signer is not a compressed secp256k1 key",
  );
  const appKey = recoverSigner(
    signer.app_signature,
    keccak_256(ascii(`${signer.purpose}:${signerKey}`)),
    "app signature",
  );
  const kmsSigner = recoverSigner(
    signer.kms_signature,
    keccak_256(concat(KMS_ISSUED_PREFIX, unhex(appId), unhex(appKey))),
    "KMS signature",
  );
  requireThat(
    kmsSigner === kmsK256PublicKey,
    "Attestation: endpoint signer is not certified by the governed KMS",
  );
}
/** Parses the key-provider runtime event: `{"name":"kms","id":"<hex>"}`. */
function keyProviderId(events: Record<string, string>) {
  let parsed: any;
  try {
    parsed = JSON.parse(
      new TextDecoder().decode(unhex(events["key-provider"] ?? "")),
    );
  } catch {
    parsed = undefined;
  }
  requireThat(
    parsed?.name === "kms" && typeof parsed.id === "string",
    "Attestation: quote does not name a KMS key provider",
  );
  return strip0x(parsed.id);
}

/**
 * GETs `origin + path` as JSON. Failures are classified by HTTP status and
 * error type, never by message text (a body can say anything).
 */
async function endpointJson(
  origin: string,
  path: string,
  timeoutMs: number,
  maxBytes?: number,
): Promise<any> {
  const where = `${origin}${path.split("?")[0]}`;
  try {
    return await clientJsonFetch(`${origin}${path}`, {}, timeoutMs, maxBytes);
  } catch (error) {
    if (error instanceof HttpError) {
      if (
        path.startsWith("/attestation/v2") &&
        (error.status === 404 || error.status === 405)
      )
        fail(
          "ENDPOINT_TOO_OLD",
          `${origin} does not serve /attestation/v2 (HTTP ${error.status}) yet, so the v2 check cannot run; it does not fall back to the weaker v1 check`,
        );
      fail(
        "ENDPOINT_UNREACHABLE",
        `${where} answered HTTP ${error.status}${error.detail ? `: ${error.detail}` : ""}`,
      );
    }
    if (error instanceof SyntaxError)
      fail("MALFORMED_RESPONSE", `${where} did not return JSON`);
    fail(
      "ENDPOINT_UNREACHABLE",
      `${where}: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

/**
 * Verifies that `origin` is a genuine, governed Lit TEE deployment whose boot
 * measurements are certified by the governed dstack KMS, and that it answered
 * this request. Fails closed with an `AttestationError` carrying a stable code.
 *
 * Most callers want `verifyLitEndpoint()` from the SDK, which supplies the
 * built-in policy and TLS binding automatically.
 */
export async function verifyAttestationV2(
  origin: string,
  policy: AttestationPolicy,
  options: VerifyOptions = {},
): Promise<AttestationReportV2> {
  const now = options.now ?? Date.now();
  const timeoutMs = options.timeoutMs ?? 30000;
  const { appId, kmsContract, rpcUrls } = await step("INVALID_POLICY", () => {
    requireThat(
      typeof policy?.rpcUrl === "string" && policy.rpcUrl.length > 0,
      "Attestation: policy rpcUrl is required",
    );
    const fallbacks: unknown = policy.fallbackRpcUrls ?? [];
    requireThat(
      Array.isArray(fallbacks) &&
        fallbacks.every((url) => typeof url === "string" && url.length > 0),
      "Attestation: policy fallbackRpcUrls must be a list of URLs",
    );
    return {
      appId: hexBytes(String(policy.appId ?? ""), 20, "policy appId"),
      kmsContract: hexBytes(
        String(policy.kmsContract ?? ""),
        20,
        "policy kmsContract",
      ),
      rpcUrls: [policy.rpcUrl, ...(fallbacks as string[])],
    };
  });
  // Always fresh and never caller-chosen: a reused nonce would accept a
  // recorded response.
  const nonce = randomBytes(32);
  const nonceHex = hex(nonce);
  const checks: string[] = [];
  // Start the KMS lookup alongside the endpoint fetches; its failure surfaces
  // in order below.
  const kmsInfo = step("GOVERNANCE_UNAVAILABLE", async () =>
    decodeKmsInfo(
      await ethCall(rpcUrls, kmsContract, SELECTOR_KMS_INFO, timeoutMs),
    ),
  );
  kmsInfo.catch(() => {});
  const [attestation, info] = await Promise.all([
    endpointJson(
      origin,
      `/attestation/v2?nonce=${nonceHex}`,
      timeoutMs,
      4 * 1024 * 1024,
    ),
    endpointJson(origin, "/info", timeoutMs),
  ]);
  const signer = attestation?.signer;
  if (
    attestation?.version !== 2 ||
    typeof attestation.quote !== "string" ||
    typeof attestation.event_log !== "string" ||
    typeof attestation.vm_config !== "string" ||
    typeof attestation.signature !== "string" ||
    typeof signer !== "object" ||
    signer === null
  )
    fail("MALFORMED_RESPONSE", "malformed /attestation/v2 response");
  if (attestation.nonce !== nonceHex)
    fail("STALE_OR_REPLAYED", "response does not echo this request's nonce");
  const quoteBytes = await step("QUOTE_INVALID", () =>
    unhex(strip0x(attestation.quote)),
  );
  const quote = await step("QUOTE_INVALID", () =>
    verifyQuote(attestation.quote, now),
  );
  checks.push("tdx-quote-signature-chain");
  const identity = await step("MEASURED_IDENTITY_MISMATCH", () =>
    measuredIdentity(appId, quote, attestation, info, checks),
  );
  const { events, composeHash, osImageHash, images } = identity;
  const { measurements } = quote;

  // Freshness: the signer answered this nonce for this exact quote.
  const signerKey = await step("BOOT_BINDING_FAILED", () => {
    const key = signer.public_key;
    requireThat(
      typeof key === "string" && /^0[23][0-9a-f]{64}$/.test(key),
      "Attestation: endpoint signer is not a compressed secp256k1 key",
    );
    return key as string;
  });
  const statementSigner = await step("STALE_OR_REPLAYED", () =>
    recoverSigner(
      attestation.signature,
      attestationStatementDigest(nonce, quoteBytes),
      "statement signature",
    ),
  );
  if (statementSigner !== signerKey)
    fail(
      "STALE_OR_REPLAYED",
      "endpoint signature does not cover this request's nonce and quote",
    );
  checks.push("fresh-nonce-signature");

  // Governance lookups: KMS keys, compose hash and OS image.
  const [kms, composeAllowed, osAllowed] = await Promise.all([
    kmsInfo,
    step("GOVERNANCE_UNAVAILABLE", () =>
      allowedOnChain(
        rpcUrls,
        appId,
        SELECTOR_ALLOWED_COMPOSE_HASHES,
        composeHash,
        timeoutMs,
      ),
    ),
    step("GOVERNANCE_UNAVAILABLE", () =>
      allowedOnChain(
        rpcUrls,
        kmsContract,
        SELECTOR_ALLOWED_OS_IMAGES,
        osImageHash,
        timeoutMs,
      ),
    ),
  ]);
  if (!/^0[23][0-9a-f]{64}$/.test(kms.k256PublicKey) || !kms.caPublicKey)
    fail("KMS_MISMATCH", "DstackKms publishes no usable KMS keys");
  const pinned = PINNED_KMS_ROOTS[kmsContract];
  if (
    pinned &&
    (kms.k256PublicKey !== pinned.k256PublicKey ||
      kms.caPublicKey !== pinned.caPublicKey)
  )
    fail(
      "KMS_MISMATCH",
      "the KMS keys read from DstackKms differ from the ones pinned in this SDK: the Base RPC endpoint may be dishonest, or the KMS was rotated after this SDK was released",
    );

  // KMS identity: the measured key-provider is the governed KMS.
  const providerId = await step("KMS_MISMATCH", () => keyProviderId(events));
  if (providerId !== kms.caPublicKey)
    fail(
      "KMS_MISMATCH",
      "quote's key-provider is not the KMS governed by DstackKms",
    );
  checks.push("kms-identity");

  // Boot binding: the governed KMS certified this endpoint's signer.
  await step("BOOT_BINDING_FAILED", () =>
    verifyKmsSignatureChain(appId, signer, kms.k256PublicKey),
  );
  checks.push("kms-certified-boot-binding");

  if (!composeAllowed)
    fail("NOT_GOVERNED", "compose-hash is not whitelisted in DstackApp");
  if (!osAllowed)
    fail("NOT_GOVERNED", "OS image is not whitelisted in DstackKms");
  checks.push("onchain-governance");

  const report: AttestationReportV2 = {
    origin,
    appId,
    composeHash,
    osImageHash,
    instanceId: events["instance-id"] ?? "",
    mrKms: events["mr-kms"] ?? "",
    measurements,
    teeTcbSvn: quote.teeTcbSvn,
    images,
    checks,
    verifiedAt: now,
    version: 2,
    bootBinding: "kms-certified",
    freshness: { nonce: nonceHex },
    kms: {
      contract: kmsContract,
      k256PublicKey: kms.k256PublicKey,
      caPublicKey: kms.caPublicKey,
    },
    signer: signerKey,
    channel: "not-bound",
  };
  if (options.tlsCertificateSha256) {
    await step("TLS_BINDING_FAILED", () =>
      verifyTlsBinding(
        origin,
        measurements,
        options.tlsCertificateSha256!,
        now,
        timeoutMs,
      ),
    );
    report.tlsCertificateSha256 = strip0x(options.tlsCertificateSha256);
    report.channel = "tls-certificate-bound";
    checks.push("tls-certificate-in-tee");
  }
  return report;
}
/**
 * Binds the observed TLS certificate to the attested enclave: the dstack-ingress
 * evidence quote (same measurements as the app quote) commits to
 * SHA-256(sha256sum.txt), which lists SHA-256(cert PEM); that PEM's leaf DER
 * must equal the certificate the caller saw on the wire.
 */
export async function verifyTlsBinding(
  origin: string,
  measurements: TdxQuote["measurements"],
  tlsCertificateSha256: string,
  now: number,
  timeoutMs: number,
) {
  const expected = hexBytes(tlsCertificateSha256, 32, "TLS certificate hash");
  const [evidence, checksums] = await Promise.all([
    jsonFetch(`${origin}/evidences/quote.json`, {}, timeoutMs),
    textFetch(`${origin}/evidences/sha256sum.txt`, {}, timeoutMs),
  ]);
  const evidenceQuote = verifyQuote(evidence?.quote ?? "", now);
  requireThat(
    JSON.stringify(evidenceQuote.measurements) === JSON.stringify(measurements),
    "Attestation: evidence quote comes from a different enclave",
  );
  requireThat(
    evidenceQuote.reportData.slice(0, 64) === hex(sha256(ascii(checksums))) &&
      evidenceQuote.reportData.slice(64) === "0".repeat(64),
    "Attestation: evidence checksums are not bound to the quote",
  );
  const line = checksums
    .split("\n")
    .map((l) => /^([0-9a-f]{64})\s+(cert-[A-Za-z0-9.-]+\.pem)$/.exec(l.trim()))
    .find(Boolean);
  requireThat(line, "Attestation: evidence lacks a certificate entry");
  const pem = await textFetch(`${origin}/evidences/${line[2]}`, {}, timeoutMs);
  requireThat(
    hex(sha256(ascii(pem))) === line[1],
    "Attestation: evidence certificate hash mismatch",
  );
  const [leaf] = pemCertificates(pem);
  requireThat(
    leaf && hex(sha256(leaf)) === expected,
    "Attestation: TLS certificate is not the attested one",
  );
}
