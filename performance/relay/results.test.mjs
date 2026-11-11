import test from "node:test";
import assert from "node:assert/strict";
import { Wallet } from "ethers";
import { readFileSync } from "node:fs";
import { createHash, randomUUID } from "node:crypto";
import { MESSAGE, OPERATIONS, stats, validate } from "./results.mjs";
import { publicResult } from "./samples.mjs";
import { parseRun } from "./report.mjs";
import { duration, joinTraces } from "./trace-results.mjs";
const params = JSON.parse(
  readFileSync(new URL("./fixtures/sign-params.json", import.meta.url)),
);
const wallet = Wallet.createRandom(),
  other = Wallet.createRandom();
const sig = await wallet.signMessage(MESSAGE);
const body = {
  has_error: false,
  response: { address: wallet.address, signature: sig },
};
const relay = {
  has_error: false,
  response: {
    vmType: "ethereum-vm",
    address: wallet.address,
    withdrawRequestHash: params.attestation.withdrawRequestHash,
    results: params.attestation.hashesToSign.map((hash) => ({
      hash,
      signature: wallet.signingKey.sign(hash).serialized,
    })),
  },
};
function runRecords() {
  return {
    meta: {
      baseUrl: "https://example.test/core/v1",
      ...Object.fromEntries(
        [
          "relayFixtureSha256",
          "paramsSha256",
          "benchmarkSha256",
          "clientSha256",
        ].map((k) => [k, "a".repeat(64)]),
      ),
      correlationId: "test",
      pkpId: wallet.address,
      startedAt: "2026-11-11T00:00:00Z",
    },
    samples: OPERATIONS.flatMap((op) =>
      Array.from({ length: 12 }, (_, index) => ({
        operation: op,
        index,
        warmup: index < 2,
        startedAt: "2026-11-11T00:00:01Z",
        clientMs: 100 + index,
        requestId: randomUUID(),
        body:
          op === "hello"
            ? { has_error: false, response: "benchmark-ok" }
            : op.startsWith("relay_")
              ? relay
              : body,
      })),
    ),
    done: { completedAt: "2026-11-11T00:01:00Z" },
  };
}
const text = (r) =>
  [
    "setup log",
    `RELAY_META ${JSON.stringify(r.meta)}`,
    ...r.samples.map((s) => `RELAY_SAMPLE ${JSON.stringify(s)}`),
    `RELAY_DONE ${JSON.stringify(r.done)}`,
    "RELAY_CLEANUP {}",
  ].join("\n");
test("fixture matches the original saved-wallet Relay benchmark", () => {
  assert.equal(
    createHash("sha256")
      .update(readFileSync(new URL("./fixtures/relay.js", import.meta.url)))
      .digest("hex"),
    "0dad112787b745f58f26cfdda51493ab1ac855a08c709fdb7e9148df391f4a54",
  );
});
test("percentiles use median and nearest rank; no samples is not zero latency", () => {
  assert.deepEqual(stats([1, 2, 3, 4]), { count: 4, medianMs: 2.5, p95Ms: 4 });
  assert.equal(stats([]).medianMs, null);
});
test("signatures must recover the expected wallet", () => {
  assert.equal(
    validate("pkp_sign", body, wallet.address, params),
    wallet.address,
  );
  assert.throws(() => validate("pkp_sign", body, other.address, params));
  assert.throws(() =>
    validate(
      "pkp_sign",
      {
        ...body,
        response: {
          ...body.response,
          signature: other.signingKey.sign("0x" + "aa".repeat(32)).serialized,
        },
      },
      wallet.address,
      params,
    ),
  );
});
test("Relay checks attestation hash, every digest, signature, and address", () => {
  assert.equal(
    validate("relay_sign", relay, wallet.address, params),
    wallet.address,
  );
  for (const patch of [
    { withdrawRequestHash: "0x" + "bb".repeat(32) },
    { results: [] },
    { address: other.address },
    {
      results: [{ ...relay.response.results[0], hash: "0x" + "cc".repeat(32) }],
    },
  ])
    assert.throws(() =>
      validate(
        "relay_sign",
        { ...relay, response: { ...relay.response, ...patch } },
        wallet.address,
        params,
      ),
    );
});
test("public sample projection strips logs/unknown fields and rejects failures", () => {
  assert.deepEqual(
    publicResult("pkp_sign", {
      ...body,
      logs: "must not be copied",
      secret: "not a result",
      response: { ...body.response, extra: "not copied" },
    }),
    body,
  );
  assert.throws(() =>
    publicResult("hello", { has_error: true, response: "benchmark-ok" }),
  );
  assert.throws(() =>
    publicResult("hello", {
      has_error: false,
      response: { unexpected: "data" },
    }),
  );
});
test("complete report excludes warmups and preserves exact request IDs", () => {
  const r = runRecords(),
    report = parseRun(text(r), params);
  assert.equal(report.samples.length, 60);
  assert.equal(report.measurements.relay_sign.count, 10);
  assert.equal(report.measurements.relay_sign.medianMs, 106.5);
  assert.equal(report.samples[0].requestId, r.samples[0].requestId);
  assert.equal("body" in report.samples[0], false);
});
test("partial, duplicate, reordered and failed benchmark output cannot pass", () => {
  for (const mutate of [
    (r) => r.samples.pop(),
    (r) => r.samples.push(r.samples[0]),
    (r) => r.samples.reverse(),
    (r) => (r.samples[1].requestId = r.samples[0].requestId),
    (r) => (r.samples[2].body.has_error = true),
  ]) {
    const r = structuredClone(runRecords());
    mutate(r);
    assert.throws(() => parseRun(text(r), params));
  }
  assert.throws(() =>
    parseRun(text(runRecords()).replace("RELAY_DONE", "NOT_DONE"), params),
  );
  assert.throws(() =>
    parseRun(text(runRecords()).replace("RELAY_CLEANUP", "NO_CLEANUP"), params),
  );
});
function trace(
  id,
  start = "2026-11-11T00:00:01.000000001Z",
  end = "2026-11-11T00:00:01.080000001Z",
) {
  return {
    traceId: id,
    spans: [
      {
        spanId: "1",
        name: "http_request",
        startTime: start,
        endTime: end,
        labels: {
          request_id: "request-a",
          uri: "/core/v1/lit_action",
          "service.name": "lit-api-server",
        },
      },
      {
        spanId: "2",
        parentSpanId: "1",
        name: "get_private_key",
        startTime: start,
        endTime: end,
      },
      { spanId: "3", name: "get_private_key", startTime: start, endTime: end },
    ],
  };
}
const input = {
  samples: [
    {
      requestId: "request-a",
      operation: "pkp_sign",
      clientMs: 100,
      warmup: false,
      ok: true,
    },
  ],
};
test("nanosecond durations and exact joins; unrelated children are excluded", () => {
  const t = trace("trace-a");
  assert.equal(duration(t.spans[0]), 80);
  const r = joinTraces(input, [t, t]);
  assert.equal(r.telemetry.status, "complete");
  assert.equal(r.samples[0].outsideServerMs, 20);
  assert.equal(r.samples[0].spans.get_private_key.count, 1);
  assert.equal(
    r.samples[0].spans["accounts::get_account_wallet_address"].totalMs,
    null,
  );
});
test("missing, ambiguous, capped, and impossible spans stay explicit", () => {
  assert.equal(joinTraces(input, []).samples[0].telemetryStatus, "missing");
  assert.equal(
    joinTraces(input, [trace("a"), trace("b")]).samples[0].telemetryStatus,
    "ambiguous",
  );
  assert.equal(
    joinTraces(input, [trace("a")], true).telemetry.status,
    "incomplete",
  );
  assert.equal(
    joinTraces({ ...input, samples: [{ ...input.samples[0], clientMs: 1 }] }, [
      trace("a"),
    ]).samples[0].telemetryStatus,
    "invalid-duration",
  );
});
