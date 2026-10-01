import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import {
  verifyAttestation,
  verifyQuote,
  replayEventLog,
  CHIPOTLE_ATTESTATION_POLICY,
  BASE_PUBLIC_RPC_URLS,
  LitConnection,
  Keychain,
} from "../sdk/src/index.ts";
import { pinnedImages } from "../protocol/attestation.ts";
import { json } from "./harness.ts";

// Captured from https://api.chipotle.litprotocol.com on 2026-09-11. The quote,
// PCK chain and evidence are real Intel-signed artifacts; the tests pin `now`
// inside the PCK certificates' validity window.
const DIR = "tests/fixtures/attestation/";
const read = (name: string) => readFileSync(DIR + name, "utf8");
const attestation = JSON.parse(read("attestation.json"));
const info = JSON.parse(read("info.json"));
const evidenceQuote = JSON.parse(read("evidence-quote.json"));
const checksums = read("sha256sum.txt");
const certPem = read("cert-api.chipotle.litprotocol.com.pem");
const tcbInfo =
  typeof info.tcb_info === "string" ? JSON.parse(info.tcb_info) : info.tcb_info;
const NOW = Date.UTC(2026, 8, 11);
const ORIGIN = "https://attested.example";
const TRUE = "0x" + "0".repeat(63) + "1";
const FALSE = "0x" + "0".repeat(64);
const leafDer = Buffer.from(
  certPem.match(
    /-----BEGIN CERTIFICATE-----([^-]+)-----END CERTIFICATE-----/,
  )![1],
  "base64",
);
const liveCertHash = createHash("sha256").update(leafDer).digest("hex");

type Overrides = Partial<{
  attestation: unknown;
  info: unknown;
  evidence: unknown;
  checksums: string;
  cert: string;
  rpc: (data: string) => string;
  /** Per-endpoint responder; wins over `rpc` for the matching URL. */
  rpcByUrl: Record<string, (data: string) => Response>;
}>;
/** Serves the fixtures and a fake Base RPC; records every call. */
function serve(overrides: Overrides = {}) {
  const calls: string[] = [];
  const original = globalThis.fetch;
  globalThis.fetch = async (input, init) => {
    const url = String(input);
    calls.push(url);
    const text = (body: string) =>
      new Response(body, {
        status: 200,
        headers: { "content-type": "text/plain" },
      });
    if (url === `${ORIGIN}/attestation`)
      return json(overrides.attestation ?? attestation);
    if (url === `${ORIGIN}/info`) return json(overrides.info ?? info);
    if (url === `${ORIGIN}/evidences/quote.json`)
      return json(overrides.evidence ?? evidenceQuote);
    if (url === `${ORIGIN}/evidences/sha256sum.txt`)
      return text(overrides.checksums ?? checksums);
    if (url === `${ORIGIN}/evidences/cert-api.chipotle.litprotocol.com.pem`)
      return text(overrides.cert ?? certPem);
    if (overrides.rpcByUrl?.[url]) {
      const { params } = JSON.parse(String(init?.body));
      return overrides.rpcByUrl[url](params[0].data);
    }
    if (url === CHIPOTLE_ATTESTATION_POLICY.rpcUrl) {
      const { params } = JSON.parse(String(init?.body));
      const data: string = params[0].data;
      const result = overrides.rpc ? overrides.rpc(data) : TRUE;
      return json({ jsonrpc: "2.0", id: 1, result });
    }
    return json({ error: "not_found" }, 404);
  };
  return { calls, restore: () => (globalThis.fetch = original) };
}

test("verifies a real Chipotle TDX quote, event log, governance and TLS binding offline", async () => {
  const quote = verifyQuote(attestation.quote, NOW);
  const tcb = tcbInfo;
  assert.equal(quote.measurements.mrtd, tcb.mrtd);
  assert.equal(quote.measurements.rtmr3, tcb.rtmr3);
  assert.equal(quote.reportData, "0".repeat(128));
  const replay = replayEventLog(attestation.event_log);
  assert.deepEqual(replay.rtmrs, [tcb.rtmr0, tcb.rtmr1, tcb.rtmr2, tcb.rtmr3]);
  assert.equal(replay.events["app-id"], CHIPOTLE_ATTESTATION_POLICY.appId);
  assert.equal(pinnedImages(tcb.app_compose).length, 4);
  const { calls, restore } = serve();
  try {
    const report = await verifyAttestation(
      ORIGIN,
      CHIPOTLE_ATTESTATION_POLICY,
      { now: NOW, tlsCertificateSha256: liveCertHash },
    );
    assert.equal(report.composeHash, info.compose_hash);
    assert.equal(report.osImageHash, info.os_image_hash);
    assert.deepEqual(report.checks, [
      "tdx-quote-signature-chain",
      "event-log-replay",
      "measured-identity",
      "images-digest-pinned",
      "onchain-governance",
      "tls-certificate-in-tee",
    ]);
    // Both governance lookups hit the chain with the measured hashes.
    const rpcCalls = calls.filter(
      (u) => u === CHIPOTLE_ATTESTATION_POLICY.rpcUrl,
    );
    assert.equal(rpcCalls.length, 2);
  } finally {
    restore();
  }
});

test("attestation fails closed on every tampered input", async () => {
  const flip = (hexString: string, index: number) =>
    hexString.slice(0, index) +
    (hexString[index] === "0" ? "1" : "0") +
    hexString.slice(index + 1);
  const cases: [string, Overrides | { now: number }, RegExp][] = [
    [
      "quote body bit flipped",
      { attestation: { ...attestation, quote: flip(attestation.quote, 700) } },
      /quote signature invalid/,
    ],
    [
      "quote signature bit flipped",
      { attestation: { ...attestation, quote: flip(attestation.quote, 1300) } },
      /quote signature invalid/,
    ],
    [
      "PCK certificate bit flipped",
      { attestation: { ...attestation, quote: flip(attestation.quote, 4000) } },
      /Attestation:/,
    ],
    [
      "event log payload changed",
      {
        attestation: {
          ...attestation,
          event_log: attestation.event_log.replace(
            /"compose-hash","event_payload":"a3/,
            '"compose-hash","event_payload":"b3',
          ),
        },
      },
      // dstack serves runtime events with empty digests, so a forged payload
      // is caught when the replayed RTMR3 diverges from the quoted one.
      /does not reproduce the quoted RTMRs|digest mismatch/,
    ],
    [
      "event log entry dropped",
      {
        attestation: {
          ...attestation,
          event_log: JSON.stringify(
            JSON.parse(attestation.event_log).filter(
              (e: any) => e.event !== "storage-fs",
            ),
          ),
        },
      },
      /does not reproduce the quoted RTMRs/,
    ],
    [
      "served app_compose differs from the measured one",
      {
        info: {
          ...info,
          // tcb_info may be served as a JSON string or an object; cover both.
          tcb_info: JSON.stringify({
            ...tcbInfo,
            app_compose: tcbInfo.app_compose + " ",
          }),
        },
      },
      /does not hash to the measured compose-hash/,
    ],
    [
      "compose hash not whitelisted",
      { rpc: (data) => (data.startsWith("0x2f6622e5") ? FALSE : TRUE) },
      /compose-hash is not whitelisted/,
    ],
    [
      "OS image not whitelisted",
      { rpc: (data) => (data.startsWith("0x9a4e1d18") ? FALSE : TRUE) },
      /OS image is not whitelisted/,
    ],
    ["PCK certificate expired", { now: Date.UTC(2040, 0, 1) }, /expired/],
    [
      "evidence checksums replaced",
      { checksums: checksums.replace("bdbca9", "bdbca8") },
      /not bound to the quote/,
    ],
    [
      "evidence certificate replaced",
      {
        cert: certPem.replace(
          /\n-----END CERTIFICATE-----/,
          "A\n-----END CERTIFICATE-----",
        ),
      },
      /certificate hash mismatch/,
    ],
  ];
  for (const [name, overrides, expected] of cases) {
    const now = "now" in overrides ? overrides.now : NOW;
    const { restore } = serve("now" in overrides ? {} : overrides);
    try {
      await assert.rejects(
        verifyAttestation(ORIGIN, CHIPOTLE_ATTESTATION_POLICY, {
          now,
          tlsCertificateSha256: liveCertHash,
        }),
        expected,
        name,
      );
    } finally {
      restore();
    }
  }
  // Wrong app pinned in policy.
  const { restore } = serve();
  try {
    await assert.rejects(
      verifyAttestation(
        ORIGIN,
        { ...CHIPOTLE_ATTESTATION_POLICY, appId: "00".repeat(20) },
        { now: NOW },
      ),
      /different dstack app/,
    );
    // Observed TLS certificate is not the attested one.
    await assert.rejects(
      verifyAttestation(ORIGIN, CHIPOTLE_ATTESTATION_POLICY, {
        now: NOW,
        tlsCertificateSha256: "11".repeat(32),
      }),
      /TLS certificate is not the attested one/,
    );
  } finally {
    restore();
  }
});

test("LitConnection attests known origins before the first request and caches the result", async () => {
  const { calls, restore } = serve();
  try {
    const pinned = new LitConnection(
      ORIGIN,
      5000,
      "usage-key",
      CHIPOTLE_ATTESTATION_POLICY,
    );
    // The fixture PCK chain is valid today; no `now` override is available on
    // the connection, so this exercises the real clock path.
    const first = await pinned.attest();
    const second = await pinned.attest();
    assert.ok(first && first === second, "attestation result is cached");
    assert.equal(calls.filter((u) => u.endsWith("/attestation")).length, 1);
    // Unknown origins are not attested; disabled connections skip the check.
    const local = new LitConnection("http://localhost:8000", 5000, "k");
    assert.equal(await local.attest(), undefined);
    const off = new LitConnection(ORIGIN, 5000, "k", false);
    assert.equal(await off.attest(), undefined);
    // The default production origin is attested unless explicitly disabled.
    assert.ok(new LitConnection().attestationPolicy);
    const agent = new Keychain(Keychain.generateKey().privateKey, {
      v: 2,
      litApiUrl: ORIGIN,
      usageApiKey: "usage-key",
      secrets: {},
    });
    assert.equal(agent.lit.attestationPolicy, undefined);
    agent.destroy();
  } finally {
    restore();
  }
});

test("a failed attestation blocks execution and is retried, not cached", async () => {
  let allowed = false;
  const { restore } = serve({ rpc: () => (allowed ? TRUE : FALSE) });
  try {
    const lit = new LitConnection(
      ORIGIN,
      5000,
      "usage-key",
      CHIPOTLE_ATTESTATION_POLICY,
    );
    await assert.rejects(
      lit.publicKey("bafkreib" + "a".repeat(51)),
      /not whitelisted/,
    );
    allowed = true;
    assert.ok(await lit.attest());
  } finally {
    restore();
  }
});

test("the governance check rotates through fallback RPCs and names every failure", async () => {
  const [primary, second, third] = BASE_PUBLIC_RPC_URLS;
  assert.deepEqual(CHIPOTLE_ATTESTATION_POLICY.fallbackRpcUrls, [
    second,
    third,
    BASE_PUBLIC_RPC_URLS[3],
  ]);
  const answer = (result: string) => json({ jsonrpc: "2.0", id: 1, result });
  // Rate limited primary, RPC-level error on the second, third answers.
  let served = serve({
    rpcByUrl: {
      [primary]: () => json({ message: "too many requests" }, 429),
      [second]: () =>
        json({ jsonrpc: "2.0", id: 1, error: { message: "upstream down" } }),
      [third]: () => answer(TRUE),
    },
  });
  try {
    const report = await verifyAttestation(
      ORIGIN,
      CHIPOTLE_ATTESTATION_POLICY,
      { now: NOW },
    );
    assert.ok(report.checks.includes("onchain-governance"));
    const hosts = (u: string) => new URL(u).host;
    for (const url of [primary, second, third])
      assert.equal(
        served.calls.filter((u) => u === url).length,
        2,
        `${hosts(url)} is asked once per lookup`,
      );
    assert.equal(
      served.calls.filter((u) => u === BASE_PUBLIC_RPC_URLS[3]).length,
      0,
    );
  } finally {
    served.restore();
  }
  // A definitive "not whitelisted" from the first endpoint that answers is final.
  served = serve({
    rpcByUrl: {
      [primary]: () => json({}, 503),
      [second]: () => answer(FALSE),
      [third]: () => answer(TRUE),
    },
  });
  try {
    await assert.rejects(
      verifyAttestation(ORIGIN, CHIPOTLE_ATTESTATION_POLICY, { now: NOW }),
      /not whitelisted/,
    );
    assert.equal(served.calls.filter((u) => u === third).length, 0);
  } finally {
    served.restore();
  }
  // Every endpoint failing produces one actionable message.
  served = serve({
    rpcByUrl: Object.fromEntries(
      BASE_PUBLIC_RPC_URLS.map((u) => [u, () => json({}, 429)]),
    ),
  });
  try {
    await assert.rejects(
      verifyAttestation(ORIGIN, CHIPOTLE_ATTESTATION_POLICY, { now: NOW }),
      (error: Error) => {
        assert.match(
          error.message,
          /^Attestation: no Base RPC endpoint answered/,
        );
        assert.match(error.message, /mainnet\.base\.org: 429 rate limited/);
        assert.match(error.message, /KEYCHAIN_BASE_RPC_URL/);
        return true;
      },
    );
  } finally {
    served.restore();
  }
  // An operator-pinned endpoint with no fallbacks is the only one consulted.
  const pinned = "https://rpc.example/base";
  served = serve({ rpcByUrl: { [pinned]: () => answer(TRUE) } });
  try {
    await verifyAttestation(
      ORIGIN,
      { ...CHIPOTLE_ATTESTATION_POLICY, rpcUrl: pinned, fallbackRpcUrls: [] },
      { now: NOW },
    );
    assert.equal(served.calls.filter((u) => u === pinned).length, 2);
    assert.ok(!served.calls.some((u) => BASE_PUBLIC_RPC_URLS.includes(u)));
  } finally {
    served.restore();
  }
});
