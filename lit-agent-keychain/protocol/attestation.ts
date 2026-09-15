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
//   4. Confirm on Base that the compose-hash is whitelisted in DstackApp and the
//      OS image hash is whitelisted in DstackKms (governed by the Lit Safe).
//   5. Optionally bind the live TLS certificate to the enclave through the
//      dstack-ingress evidence quote (Node only; browsers cannot read peer certs).
//
// Deliberately not covered: Intel TCB status / QE identity collateral and PCK
// revocation. Those need Intel PCS lookups; the dstack-verifier performs them.
// The public /attestation quote carries no caller nonce, so freshness rests on
// the TLS binding (step 5) rather than on the quote itself.
import { p256 } from "@noble/curves/nist.js";
import { sha256, sha384 } from "@noble/hashes/sha2.js";
import { hex, unhex, requireThat } from "./crypto.ts";
import { jsonFetch, textFetch } from "./http.ts";

export type AttestationPolicy = {
  /** Expected dstack app id: the DstackApp contract address, hex without 0x. */
  appId: string;
  /** DstackKms contract that whitelists OS images. */
  kmsContract: string;
  /** JSON-RPC endpoint for the chain that hosts both contracts. */
  rpcUrl: string;
};
export const CHIPOTLE_ATTESTATION_POLICY: AttestationPolicy = {
  appId: "3f91deaf16ff7c823ee65081d6bafa1ceea05ffc",
  kmsContract: "0x2f83172a49584c017f2b256f0fb2dca14126ba9c",
  rpcUrl: "https://mainnet.base.org",
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
async function allowedOnChain(
  rpcUrl: string,
  contract: string,
  selector: string,
  hash32: string,
  timeoutMs: number,
) {
  const to = "0x" + hexBytes(contract, 20, "contract address");
  const result = await jsonFetch(
    rpcUrl,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "eth_call",
        params: [{ to, data: "0x" + selector + hash32 }, "latest"],
      }),
    },
    timeoutMs,
  );
  requireThat(result?.error === undefined, "Attestation: chain RPC error");
  return result?.result === TRUE_WORD;
}

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
  const [composeAllowed, osAllowed] = await Promise.all([
    allowedOnChain(
      policy.rpcUrl,
      appId,
      SELECTOR_ALLOWED_COMPOSE_HASHES,
      composeHash,
      timeoutMs,
    ),
    allowedOnChain(
      policy.rpcUrl,
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
