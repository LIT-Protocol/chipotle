import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { OPERATIONS, validate, summarize } from "./results.mjs";
import { publicResult } from "./samples.mjs";

export function parseRun(text, params) {
  const records = { META: [], SAMPLE: [], DONE: [], CLEANUP: [] };
  for (const line of text.split("\n")) {
    const match = /^RELAY_(META|SAMPLE|DONE|CLEANUP) (.*)$/.exec(line.trim());
    if (match) records[match[1]].push(JSON.parse(match[2]));
  }
  if (
    records.META.length !== 1 ||
    records.DONE.length !== 1 ||
    records.CLEANUP.length !== 1 ||
    records.SAMPLE.length !== 60
  )
    throw new Error(
      "Incomplete or duplicate benchmark run; expected metadata, completion, successful cleanup, and 60 samples",
    );
  const meta = records.META[0];
  if (
    !/^0x[0-9a-f]{40}$/i.test(meta.pkpId) ||
    typeof meta.baseUrl !== "string" ||
    !meta.correlationId
  )
    throw new Error("Invalid benchmark metadata");
  for (const key of [
    "relayFixtureSha256",
    "paramsSha256",
    "benchmarkSha256",
    "clientSha256",
  ]) {
    if (!/^[0-9a-f]{64}$/.test(meta[key] ?? ""))
      throw new Error("Missing benchmark provenance");
  }
  const samples = records.SAMPLE,
    seen = new Set(),
    expected = { pkp_sign: meta.pkpId };
  for (let i = 0; i < samples.length; i++) {
    const s = samples[i],
      op = OPERATIONS[Math.floor(i / 12)];
    if (
      s.operation !== op ||
      s.index !== i % 12 ||
      s.warmup !== i % 12 < 2 ||
      !Number.isFinite(s.clientMs) ||
      s.clientMs < 0 ||
      !Number.isFinite(Date.parse(s.startedAt)) ||
      !/^[0-9a-f-]{36}$/i.test(s.requestId ?? "") ||
      seen.has(s.requestId)
    )
      throw new Error("Invalid sample ordering, timing, or request ID");
    seen.add(s.requestId);
    const address = validate(
      op,
      publicResult(op, s.body),
      expected[op],
      params,
    );
    if (address) expected[op] = address;
    if (op === "relay_wallet") expected.relay_sign = address;
  }
  // Persist only timing/correlation metadata; discard action output and signatures.
  const safe = samples.map(
    ({ operation, index, warmup, startedAt, clientMs, requestId }) => ({
      operation,
      index,
      warmup,
      startedAt,
      clientMs,
      requestId,
      ok: true,
    }),
  );
  return {
    schemaVersion: 1,
    status: "passed",
    baseUrl: meta.baseUrl,
    correlationId: meta.correlationId,
    pkpId: meta.pkpId,
    nodeVersion: meta.nodeVersion ?? null,
    relayFixtureSha256: meta.relayFixtureSha256,
    paramsSha256: meta.paramsSha256,
    benchmarkSha256: meta.benchmarkSha256,
    clientSha256: meta.clientSha256,
    actionHashes: meta.actionHashes,
    startedAt: meta.startedAt,
    completedAt: records.DONE[0].completedAt,
    warmups: 2,
    sampleCount: 10,
    method:
      "sequential k6 generated-client requests through JSON parsing; millisecond clock; validation outside timer; no measured retries",
    measurements: summarize(safe),
    samples: safe,
  };
}
export function markdown(r) {
  return [
    "## Sequential action benchmark",
    "",
    `Result: **${r.status}**. Target: \`${String(r.baseUrl ?? "unknown").replaceAll("`", "")}\`.`,
    "",
    "| Operation | Samples | Median ms | p95 ms |",
    "| --- | ---: | ---: | ---: |",
    ...Object.entries(r.measurements ?? {}).map(
      ([op, v]) =>
        `| ${op} | ${v.count} | ${v.medianMs?.toFixed(2) ?? "n/a"} | ${v.p95Ms?.toFixed(2) ?? "n/a"} |`,
    ),
    "",
    "Two warm-ups and ten measured sequential requests per operation, including network/client latency. Not a load test. No historical latency gate or baseline refresh.",
    "Request IDs in relay-performance.json can be joined to GCP spans with performance/relay/traces.mjs. Missing telemetry is not zero latency.",
    ...(r.error ? ["", `Error: ${r.error}`] : []),
    "",
  ].join("\n");
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  const [input = "relay-samples.log", output = "relay-performance.json"] =
    process.argv.slice(2);
  let report;
  try {
    const params = JSON.parse(
      readFileSync(new URL("./fixtures/sign-params.json", import.meta.url)),
    );
    report = parseRun(readFileSync(input, "utf8"), params);
    report.commit = process.env.GITHUB_SHA ?? null;
  } catch (e) {
    report = { schemaVersion: 1, status: "failed", error: e.message };
    process.exitCode = 1;
  }
  writeFileSync(output, JSON.stringify(report, null, 2) + "\n");
  console.log(markdown(report));
  if (process.env.GITHUB_STEP_SUMMARY)
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown(report));
}
