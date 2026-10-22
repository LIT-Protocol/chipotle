import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { secp256k1 } from "@noble/curves/secp256k1.js";
import { keccak_256 } from "@noble/hashes/sha3.js";
import {
  verifyAttestation,
  verifyAttestationV2,
  verifyLitEndpoint,
  AttestationError,
  ATTESTED_ORIGINS,
  CHIPOTLE_ATTESTATION_POLICY,
  BASE_PUBLIC_RPC_URLS,
  DEFAULT_LIT_API_URL,
  LitConnection,
} from "../sdk/src/index.ts";
import {
  attestationStatementDigest,
  decodeKmsInfo,
  verifyKmsSignatureChain,
  ATTESTATION_V2_SIGNER_PURPOSE,
  PINNED_KMS_ROOTS,
  type AttestationErrorCode,
} from "../protocol/attestation.ts";
import { json } from "./harness.ts";

// The quote, event log, PCK chain and TLS evidence are the real Intel-signed
// artifacts captured from https://api.chipotle.litprotocol.com on 2026-09-11
// (see attestation.test.ts). The KMS, app and signer keys below are test keys
// that sign in exactly dstack v0.5.8's formats:
//   KMS:    keccak256("dstack-kms-issued:" || app_id || app_pubkey_sec1)
//   app:    keccak256(purpose + ":" + hex(signer_pubkey_sec1))
//   signer: keccak256("chipotle-attestation-v2:" || nonce || keccak256(quote))
// each a 65-byte r || s || v signature.
const DIR = "tests/fixtures/attestation/";
const read = (name: string) => readFileSync(DIR + name, "utf8");
const attestation = JSON.parse(read("attestation.json"));
const info = JSON.parse(read("info.json"));
const evidenceQuote = JSON.parse(read("evidence-quote.json"));
const checksums = read("sha256sum.txt");
const certPem = read("cert-api.chipotle.litprotocol.com.pem");
const liveKmsInfo = JSON.parse(read("kms-info.json")).result as string;
const NOW = Date.UTC(2026, 8, 11);
const ORIGIN = "https://attested.example";
const TRUE = "0x" + "0".repeat(63) + "1";
const FALSE = "0x" + "0".repeat(64);
const SEL_KMS_INFO = "0x09177063";
const leafDer = Buffer.from(
  certPem.match(
    /-----BEGIN CERTIFICATE-----([^-]+)-----END CERTIFICATE-----/,
  )![1],
  "base64",
);
const liveCertHash = createHash("sha256").update(leafDer).digest("hex");

const hex = (b: Uint8Array) => Buffer.from(b).toString("hex");
const unhex = (s: string) => new Uint8Array(Buffer.from(s, "hex"));
const utf8 = (s: string) => new TextEncoder().encode(s);
const cat = (...parts: Uint8Array[]) => new Uint8Array(Buffer.concat(parts));
const key = (byte: number) => new Uint8Array(32).fill(byte);
const pub = (sk: Uint8Array) => hex(secp256k1.getPublicKey(sk, true));
/** dstack's r || s || v recoverable signature over a 32-byte digest. */
function dstackSign(sk: Uint8Array, digest: Uint8Array) {
  const sig = secp256k1.sign(digest, sk, {
    prehash: false,
    format: "recovered",
  });
  return hex(cat(sig.subarray(1), sig.subarray(0, 1)));
}

const KMS = key(0x4b);
const APP = key(0x41);
const SIGNER = key(0x53);
const APP_ID = CHIPOTLE_ATTESTATION_POLICY.appId;
// The test KMS key above cannot sign for Phala's real KMS, whose root is pinned
// in the SDK. Success paths therefore name an unpinned test KMS contract (whose
// root the fake RPC serves); the pin itself is tested against the real one.
const TEST_KMS_CONTRACT = "0x" + "c0".repeat(20);
const TEST_POLICY = {
  ...CHIPOTLE_ATTESTATION_POLICY,
  kmsContract: TEST_KMS_CONTRACT,
};
const keyProvider = JSON.parse(
  Buffer.from(
    JSON.parse(attestation.event_log).find(
      (e: any) => e.event === "key-provider",
    ).event_payload,
    "hex",
  ).toString(),
);
const CA_PUBLIC_KEY: string = keyProvider.id;

/** ABI-encodes the kmsInfo() getter: (bytes k256, bytes ca, bytes quote, bytes log). */
function encodeKmsInfo(k256: string, ca: string) {
  const word = (n: number) => n.toString(16).padStart(64, "0");
  const enc = (h: string) =>
    word(h.length / 2) + h.padEnd(Math.ceil(h.length / 64) * 64, "0");
  const parts = [k256, ca, "", ""].map(enc);
  let offset = 4 * 32;
  const heads = parts.map((p) => {
    const head = word(offset);
    offset += p.length / 2;
    return head;
  });
  return "0x" + heads.join("") + parts.join("");
}

type Chain = {
  public_key: string;
  purpose: string;
  app_signature: string;
  kms_signature: string;
};
function chain({
  kms = KMS,
  app = APP,
  signer = SIGNER,
  appId = APP_ID,
  purpose = ATTESTATION_V2_SIGNER_PURPOSE,
} = {}): Chain {
  const signerPub = pub(signer);
  return {
    public_key: signerPub,
    purpose,
    app_signature: dstackSign(app, keccak_256(utf8(`${purpose}:${signerPub}`))),
    kms_signature: dstackSign(
      kms,
      keccak_256(
        cat(utf8("dstack-kms-issued:"), unhex(appId), unhex(pub(app))),
      ),
    ),
  };
}

type V2Overrides = Partial<{
  /** Mutates the /attestation/v2 body after it is signed. */
  tamper: (body: any, nonce: string) => void;
  signer: Uint8Array;
  signedNonce: (nonce: string) => string;
  signedQuote: string;
  chain: Chain;
  kmsInfo: string;
  rpc: (data: string) => string;
  rpcByUrl: Record<string, (data: string) => Response>;
  v2Status: number;
  /** Raw 200 body for /attestation/v2. */
  v2Raw: string;
}>;
/** Serves the real fixtures behind a v2 endpoint plus a fake Base RPC. */
function serve(origin: string, o: V2Overrides = {}) {
  const calls: string[] = [];
  const original = globalThis.fetch;
  globalThis.fetch = async (input, init) => {
    const url = String(input);
    calls.push(url);
    const text = (body: string) => new Response(body, { status: 200 });
    const v2 = url.match(/\/attestation\/v2\?nonce=([0-9a-f]*)$/);
    if (url.startsWith(origin) && v2) {
      // The body names a 404 so status, not text, must drive classification.
      if (o.v2Status) return json({ error: "upstream said (404)" }, o.v2Status);
      if (o.v2Raw !== undefined) return text(o.v2Raw);
      const nonce = v2[1];
      const signedNonce = o.signedNonce ? o.signedNonce(nonce) : nonce;
      const quote = o.signedQuote ?? attestation.quote;
      const body: any = {
        version: 2,
        nonce,
        quote: attestation.quote,
        event_log: attestation.event_log,
        vm_config: attestation.vm_config,
        signer: o.chain ?? chain(),
        signature: dstackSign(
          o.signer ?? SIGNER,
          attestationStatementDigest(unhex(signedNonce), unhex(quote)),
        ),
      };
      o.tamper?.(body, nonce);
      return json(body);
    }
    if (url === `${origin}/info`) return json(info);
    if (url === `${origin}/evidences/quote.json`) return json(evidenceQuote);
    if (url === `${origin}/evidences/sha256sum.txt`) return text(checksums);
    if (url === `${origin}/evidences/cert-api.chipotle.litprotocol.com.pem`)
      return text(certPem);
    if (url === `${origin}/attestation`) return json(attestation);
    if (url.startsWith("http") && init?.method === "POST") {
      const { params } = JSON.parse(String(init?.body));
      const data: string = params[0].data;
      if (o.rpcByUrl?.[url]) return o.rpcByUrl[url](data);
      const result = data.startsWith(SEL_KMS_INFO)
        ? (o.kmsInfo ?? encodeKmsInfo(pub(KMS), CA_PUBLIC_KEY))
        : o.rpc
          ? o.rpc(data)
          : TRUE;
      return json({ jsonrpc: "2.0", id: 1, result });
    }
    return json({ error: "not_found" }, 404);
  };
  return { calls, restore: () => (globalThis.fetch = original) };
}
const v2 = (o: V2Overrides = {}, tls?: string, policy = TEST_POLICY) => {
  const served = serve(ORIGIN, o);
  return verifyAttestationV2(ORIGIN, policy, {
    now: NOW,
    tlsCertificateSha256: tls,
  }).finally(served.restore);
};
const rejectsWith = async (
  promise: Promise<unknown>,
  code: AttestationErrorCode,
  message: RegExp,
  name: string,
) =>
  assert.rejects(
    promise,
    (error: unknown) => {
      assert.ok(error instanceof AttestationError, `${name}: AttestationError`);
      assert.equal(error.code, code, `${name}: ${error.message}`);
      assert.match(error.message, /^Attestation: /, name);
      assert.match(error.message, message, name);
      return true;
    },
    name,
  );

test("decodes the real DstackKms kmsInfo() and it names the fixture quote's KMS", () => {
  const kms = decodeKmsInfo(liveKmsInfo);
  assert.equal(
    kms.k256PublicKey,
    "02a335aea3df11d46ae0a373d3f26fbd1d208572ecc87e630e618dac693b7af721",
  );
  // The production quote's key-provider event names exactly the on-chain KMS.
  assert.equal(kms.caPublicKey, CA_PUBLIC_KEY);
  assert.throws(() => decodeKmsInfo("0x"), /malformed kmsInfo/);
  assert.throws(() => decodeKmsInfo(liveKmsInfo.slice(0, 200)), /malformed/);
  assert.throws(() => decodeKmsInfo(42), /malformed/);
});

test("the SDK pins Phala's KMS root, so a dishonest Base RPC cannot replace it", async () => {
  // The pin is exactly what DstackKms published at the fixture block, and the
  // production quote's key-provider names that same KMS.
  const pinned =
    PINNED_KMS_ROOTS[
      CHIPOTLE_ATTESTATION_POLICY.kmsContract.replace(/^0x/, "")
    ];
  assert.deepEqual(pinned, decodeKmsInfo(liveKmsInfo));
  assert.equal(pinned.caPublicKey, CA_PUBLIC_KEY);
  assert.ok(Object.isFrozen(PINNED_KMS_ROOTS) && Object.isFrozen(pinned));
  // An RPC answering with a KMS root of its own (here the test KMS that
  // certified the forged chain) fails closed, whatever else checks out.
  await rejectsWith(
    v2({}, undefined, CHIPOTLE_ATTESTATION_POLICY),
    "KMS_MISMATCH",
    /differ from the ones pinned in this SDK/,
    "RPC substitutes a KMS root",
  );
  // The address form does not matter.
  await rejectsWith(
    v2({}, undefined, {
      ...CHIPOTLE_ATTESTATION_POLICY,
      kmsContract: CHIPOTLE_ATTESTATION_POLICY.kmsContract
        .slice(2)
        .toUpperCase(),
    }),
    "KMS_MISMATCH",
    /differ from the ones pinned in this SDK/,
    "unprefixed upper-case KMS address",
  );
  // An honest RPC returns the pinned root, which never certified the test
  // signer chain.
  await rejectsWith(
    v2({ kmsInfo: liveKmsInfo }, undefined, CHIPOTLE_ATTESTATION_POLICY),
    "BOOT_BINDING_FAILED",
    /not certified by the governed KMS/,
    "genuine KMS root, forged chain",
  );
});

const STATEMENT_VECTOR_SIGNATURE =
  "4a842a94c2a19f06f4056745c42d82046a0048a908b5c779fd377e2eb547cf741b900a4a62e68ff9ce45db9640d4528382a816fdf53a78f62daee473337840df00";
test("matches the lit-api-server statement signature (cross-language vector)", () => {
  // Same vector as lit-api-server attestation_v2::tests::statement_vector_matches_the_sdk_fixture.
  const sk = key(7);
  const digest = attestationStatementDigest(
    new Uint8Array(32).fill(0x11),
    unhex("0400020081"),
  );
  assert.equal(
    pub(sk),
    "02989c0b76cb563971fdc9bef31ec06c3560f3249d6ee9e5d83c57625596e05f6f",
  );
  assert.equal(dstackSign(sk, digest), STATEMENT_VECTOR_SIGNATURE);
});

test("v2 verifies the real quote with KMS-certified boot binding, fresh nonce, KMS identity and TLS binding", async () => {
  const served = serve(ORIGIN);
  try {
    const report = await verifyAttestationV2(ORIGIN, TEST_POLICY, {
      now: NOW,
      tlsCertificateSha256: liveCertHash,
    });
    assert.deepEqual(report.checks, [
      "tdx-quote-signature-chain",
      "event-log-replay",
      "measured-identity",
      "images-digest-pinned",
      "fresh-nonce-signature",
      "kms-identity",
      "kms-certified-boot-binding",
      "onchain-governance",
      "tls-certificate-in-tee",
    ]);
    assert.equal(report.version, 2);
    assert.equal(report.bootBinding, "kms-certified");
    assert.equal(report.channel, "tls-certificate-bound");
    assert.equal(report.signer, pub(SIGNER));
    assert.equal(report.kms.k256PublicKey, pub(KMS));
    assert.equal(report.kms.caPublicKey, CA_PUBLIC_KEY);
    assert.equal(report.composeHash, info.compose_hash);
    assert.equal(report.osImageHash, info.os_image_hash);
    assert.match(report.freshness.nonce, /^[0-9a-f]{64}$/);
    // The nonce sent is the nonce proven, and each verification uses a new one.
    const asked = served.calls.filter((u) => u.includes("/attestation/v2?"));
    assert.equal(asked.length, 1);
    assert.ok(asked[0].endsWith(report.freshness.nonce));
    // Callers cannot choose the nonce (a reused one would accept a recording):
    // the old test hook is ignored.
    const fixed = new Uint8Array(32);
    const again = await verifyAttestationV2(ORIGIN, TEST_POLICY, {
      now: NOW,
      nonce: fixed,
    } as any);
    assert.notEqual(again.freshness.nonce, report.freshness.nonce);
    assert.notEqual(again.freshness.nonce, "00".repeat(32));
    assert.equal(again.channel, "not-bound");
    assert.ok(!again.checks.includes("tls-certificate-in-tee"));
    // v1 is never consulted.
    assert.ok(!served.calls.some((u) => u.endsWith("/attestation")));
  } finally {
    served.restore();
  }
});

test("v2 rejects the forged-attestation scenario that v1 accepts", async () => {
  // The reporters' attack: an attacker-controlled TDX machine serves a genuine
  // Intel-signed quote whose self-consistent event log claims Lit's app-id,
  // compose-hash and OS image. v1 has no way to tell; it accepts.
  const legacy = serve(ORIGIN);
  try {
    const report = await verifyAttestation(
      ORIGIN,
      CHIPOTLE_ATTESTATION_POLICY,
      { now: NOW },
    );
    assert.ok(report.checks.includes("onchain-governance"));
  } finally {
    legacy.restore();
  }
  // Such a machine cannot obtain a signer certified by the governed KMS for
  // Lit's app id. Every way of faking one fails v2.
  const attacker = key(0x66);
  const cases: [string, V2Overrides, AttestationErrorCode, RegExp][] = [
    [
      "self-made KMS",
      { chain: chain({ kms: attacker }) },
      "BOOT_BINDING_FAILED",
      /not certified by the governed KMS/,
    ],
    [
      "genuine KMS chain for a different app",
      { chain: chain({ appId: "11".repeat(20) }) },
      "BOOT_BINDING_FAILED",
      /not certified by the governed KMS/,
    ],
    [
      "own signer key swapped into a genuine chain",
      {
        signer: attacker,
        chain: { ...chain(), public_key: pub(attacker) },
      },
      "BOOT_BINDING_FAILED",
      /not certified by the governed KMS/,
    ],
    [
      "genuine chain copied, statement signed with another key",
      { signer: attacker },
      "STALE_OR_REPLAYED",
      /does not cover this request's nonce and quote/,
    ],
    [
      "signer certified for a different purpose",
      { chain: chain({ purpose: "client" }) },
      "BOOT_BINDING_FAILED",
      /unexpected purpose/,
    ],
    [
      "malformed KMS signature",
      { chain: { ...chain(), kms_signature: "00" } },
      "BOOT_BINDING_FAILED",
      /KMS signature must be a 65-byte/,
    ],
  ];
  for (const [name, overrides, code, message] of cases)
    await rejectsWith(v2(overrides), code, message, name);
});

test("v2 fails closed with a stable code on every tampered input", async () => {
  const flip = (hexString: string, index: number) =>
    hexString.slice(0, index) +
    (hexString[index] === "0" ? "1" : "0") +
    hexString.slice(index + 1);
  const otherNonce = "ab".repeat(32);
  const cases: [string, V2Overrides, AttestationErrorCode, RegExp][] = [
    [
      "replayed response (old nonce, echo rewritten)",
      { signedNonce: () => otherNonce },
      "STALE_OR_REPLAYED",
      /does not cover this request's nonce/,
    ],
    [
      "nonce echo mismatch",
      { tamper: (b) => (b.nonce = otherNonce) },
      "STALE_OR_REPLAYED",
      /does not echo this request's nonce/,
    ],
    [
      "signature covers a different quote",
      { signedQuote: flip(attestation.quote, 700) },
      "STALE_OR_REPLAYED",
      /does not cover this request's nonce and quote/,
    ],
    [
      "quote body bit flipped after signing",
      { tamper: (b) => (b.quote = flip(b.quote, 700)) },
      "QUOTE_INVALID",
      /quote signature invalid/,
    ],
    [
      "event log entry dropped",
      {
        tamper: (b) =>
          (b.event_log = JSON.stringify(
            JSON.parse(b.event_log).filter(
              (e: any) => e.event !== "storage-fs",
            ),
          )),
      },
      "MEASURED_IDENTITY_MISMATCH",
      /does not reproduce the quoted RTMRs/,
    ],
    [
      "quote names a different KMS than DstackKms",
      { kmsInfo: encodeKmsInfo(pub(KMS), "3059" + "00".repeat(89)) },
      "KMS_MISMATCH",
      /key-provider is not the KMS governed by DstackKms/,
    ],
    [
      "DstackKms publishes no k256 key",
      { kmsInfo: encodeKmsInfo("", CA_PUBLIC_KEY) },
      "KMS_MISMATCH",
      /no usable KMS keys/,
    ],
    [
      "compose hash not whitelisted",
      { rpc: (d) => (d.startsWith("0x2f6622e5") ? FALSE : TRUE) },
      "NOT_GOVERNED",
      /compose-hash is not whitelisted/,
    ],
    [
      "OS image not whitelisted",
      { rpc: (d) => (d.startsWith("0x9a4e1d18") ? FALSE : TRUE) },
      "NOT_GOVERNED",
      /OS image is not whitelisted/,
    ],
    [
      "every RPC down",
      {
        rpcByUrl: Object.fromEntries(
          BASE_PUBLIC_RPC_URLS.map((u) => [u, () => json({}, 429)]),
        ),
      },
      "GOVERNANCE_UNAVAILABLE",
      /no Base RPC endpoint answered/,
    ],
    [
      "endpoint predates v2",
      { v2Status: 404 },
      "ENDPOINT_TOO_OLD",
      /does not serve \/attestation\/v2 \(HTTP 404\).*does not fall back/,
    ],
    [
      "endpoint predates v2 (405)",
      { v2Status: 405 },
      "ENDPOINT_TOO_OLD",
      /HTTP 405/,
    ],
    // The body says "(404)"; only the status counts.
    ["endpoint error", { v2Status: 503 }, "ENDPOINT_UNREACHABLE", /HTTP 503/],
    [
      "200 whose body names a 404",
      { v2Raw: JSON.stringify({ error: "Request failed (404)" }) },
      "MALFORMED_RESPONSE",
      /malformed/,
    ],
    [
      "200 that is not JSON",
      { v2Raw: "<html>(404)</html>" },
      "MALFORMED_RESPONSE",
      /did not return JSON/,
    ],
    [
      "not a v2 response",
      { tamper: (b) => delete b.version },
      "MALFORMED_RESPONSE",
      /malformed/,
    ],
  ];
  for (const [name, overrides, code, message] of cases)
    await rejectsWith(v2(overrides), code, message, name);
  for (const [name, overrides] of [
    ["404", { v2Status: 404 }],
    ["503", { v2Status: 503 }],
  ] as const)
    await assert.rejects(v2(overrides), (error: Error) => {
      assert.doesNotMatch(error.message, /predates|downgrade|older SDK/, name);
      return true;
    });
  await rejectsWith(
    v2({}, "11".repeat(32)),
    "TLS_BINDING_FAILED",
    /TLS certificate is not the attested one/,
    "TLS certificate is not the attested one",
  );
  const served = serve(ORIGIN);
  try {
    await rejectsWith(
      verifyAttestationV2(
        ORIGIN,
        { ...TEST_POLICY, appId: "00".repeat(20) },
        { now: NOW },
      ),
      "MEASURED_IDENTITY_MISMATCH",
      /different dstack app/,
      "wrong app",
    );
    // A malformed policy is an AttestationError too, before anything is fetched.
    const before = served.calls.length;
    for (const [name, policy, message] of [
      ["app id", { ...TEST_POLICY, appId: "zz" }, /policy appId must be 20/],
      [
        "KMS contract",
        { ...TEST_POLICY, kmsContract: "0x1234" },
        /policy kmsContract must be 20/,
      ],
      ["RPC", { ...TEST_POLICY, rpcUrl: "" }, /rpcUrl is required/],
      ["missing", undefined, /rpcUrl is required/],
      [
        "fallback RPCs not a list",
        { ...TEST_POLICY, fallbackRpcUrls: 42 },
        /fallbackRpcUrls must be a list of URLs/,
      ],
      [
        "fallback RPC not a URL",
        { ...TEST_POLICY, fallbackRpcUrls: ["https://ok.example", ""] },
        /fallbackRpcUrls must be a list of URLs/,
      ],
    ] as const)
      await rejectsWith(
        verifyAttestationV2(ORIGIN, policy as any, { now: NOW }),
        "INVALID_POLICY",
        message,
        `invalid policy: ${name}`,
      );
    assert.equal(served.calls.length, before);
  } finally {
    served.restore();
  }
});

test("verifyKmsSignatureChain matches dstack's formats exactly", () => {
  const kmsPub = pub(KMS);
  verifyKmsSignatureChain(APP_ID, chain(), kmsPub);
  // `0x`-less 20-byte app id bytes are part of the KMS message.
  assert.throws(
    () => verifyKmsSignatureChain("22".repeat(20), chain(), kmsPub),
    /not certified/,
  );
  // Ethereum-style v (27/28) is accepted, as dstack clients sometimes emit it.
  const c = chain();
  const bump = (s: string) =>
    s.slice(0, 128) + (parseInt(s.slice(128), 16) + 27).toString(16);
  verifyKmsSignatureChain(
    APP_ID,
    {
      ...c,
      app_signature: bump(c.app_signature),
      kms_signature: bump(c.kms_signature),
    },
    kmsPub,
  );
  assert.throws(
    () =>
      verifyKmsSignatureChain(
        APP_ID,
        { ...c, public_key: "04" + "11".repeat(64) },
        kmsPub,
      ),
    /compressed secp256k1/,
  );
});

/** Temporarily replaces the built-in policy of the default Lit origin. */
async function withBuiltInPolicy<T>(
  policy: typeof TEST_POLICY,
  fn: () => Promise<T>,
) {
  const original = ATTESTED_ORIGINS[DEFAULT_LIT_API_URL];
  ATTESTED_ORIGINS[DEFAULT_LIT_API_URL] = policy;
  try {
    return await fn();
  } finally {
    ATTESTED_ORIGINS[DEFAULT_LIT_API_URL] = original;
  }
}

test("verifyLitEndpoint needs no configuration and only knows built-in origins", async () => {
  await rejectsWith(
    verifyLitEndpoint("https://impostor.example"),
    "UNKNOWN_ORIGIN",
    /No attestation policy is built in/,
    "unknown origin",
  );
  await rejectsWith(
    verifyLitEndpoint("not a url"),
    "UNKNOWN_ORIGIN",
    /Invalid URL/,
    "invalid url",
  );
  // With no options at all the default origin runs v2 under the built-in
  // policy, including the pinned KMS root: the fake RPC's test root fails.
  let served = serve(DEFAULT_LIT_API_URL);
  try {
    await rejectsWith(
      verifyLitEndpoint(),
      "KMS_MISMATCH",
      /pinned in this SDK/,
      "built-in policy pins the KMS root",
    );
    assert.ok(
      served.calls.some((u) =>
        u.startsWith(`${DEFAULT_LIT_API_URL}/attestation/v2?nonce=`),
      ),
    );
  } finally {
    served.restore();
  }
  // The rest of the default path, with the test KMS. The fixture PCK chain is
  // valid on today's clock, so the default origin verifies end to end.
  await withBuiltInPolicy(TEST_POLICY, async () => {
    served = serve(DEFAULT_LIT_API_URL);
    try {
      const report = await verifyLitEndpoint();
      assert.equal(report.origin, DEFAULT_LIT_API_URL);
      assert.equal(report.bootBinding, "kms-certified");
      // Source builds run the browser TLS stub; the Node build binds TLS.
      assert.equal(report.channel, "not-bound");
      // A fully qualified name ("host.") is the same origin, not an unknown one.
      const dotted = await verifyLitEndpoint(`${DEFAULT_LIT_API_URL}./x`);
      assert.equal(dotted.origin, DEFAULT_LIT_API_URL);
      assert.ok(!served.calls.some((u) => u.includes(".com./")));
    } finally {
      served.restore();
    }
    // An operator RPC replaces the public rotation without changing the checks.
    const pinned = "https://rpc.example/base";
    served = serve(DEFAULT_LIT_API_URL);
    try {
      const report = await verifyLitEndpoint(
        `${DEFAULT_LIT_API_URL}/some/path`,
        { rpcUrl: pinned },
      );
      assert.ok(report.checks.includes("kms-certified-boot-binding"));
      assert.equal(served.calls.filter((u) => u === pinned).length, 3);
      assert.ok(!served.calls.some((u) => BASE_PUBLIC_RPC_URLS.includes(u)));
    } finally {
      served.restore();
    }
  });
});

test("upgrading changes nothing: SDK connections keep the v1 checks for every origin", async () => {
  // Keychain, Owner, the CLI and MCP attest through LitConnection, which stays
  // exactly as released: v1 against /attestation, the origin as given and the
  // global fetch. v2 runs only where an application calls verifyLitEndpoint().
  const served = serve(DEFAULT_LIT_API_URL);
  try {
    const report = (await new LitConnection().attest()) as any;
    assert.equal(report.version, undefined);
    assert.equal(report.origin, DEFAULT_LIT_API_URL);
    // A policy passed for the built-in origin (the CLI/MCP RPC override
    // builds one) is verified with v1 as well.
    for (const policy of [
      CHIPOTLE_ATTESTATION_POLICY,
      {
        ...CHIPOTLE_ATTESTATION_POLICY,
        rpcUrl: "https://rpc.example/base",
        fallbackRpcUrls: [],
      },
    ]) {
      const lit = new LitConnection(DEFAULT_LIT_API_URL, 5000, "k", policy);
      assert.equal(((await lit.attest()) as any).version, undefined);
    }
    assert.ok(served.calls.includes(`${DEFAULT_LIT_API_URL}/attestation`));
    assert.ok(!served.calls.some((u) => u.includes("/attestation/v2")));
  } finally {
    served.restore();
  }
  // Origins are used as given, as before: a trailing-dot spelling is not
  // rewritten to the built-in origin and keeps having no built-in policy.
  const dotted = new LitConnection(`${DEFAULT_LIT_API_URL}.`);
  assert.equal(dotted.url, `${DEFAULT_LIT_API_URL}.`);
  assert.equal(dotted.attestationPolicy, undefined);
  // An origin an application registers at runtime is verified with v1 too.
  ATTESTED_ORIGINS[ORIGIN] = CHIPOTLE_ATTESTATION_POLICY;
  const legacy = serve(ORIGIN);
  try {
    const report = (await new LitConnection(ORIGIN, 5000, "k").attest()) as any;
    assert.equal(report.version, undefined);
    assert.ok(legacy.calls.includes(`${ORIGIN}/attestation`));
    assert.ok(!legacy.calls.some((u) => u.includes("/attestation/v2")));
  } finally {
    legacy.restore();
    delete ATTESTED_ORIGINS[ORIGIN];
  }
});
