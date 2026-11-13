import { OPERATIONS, stats } from "./results.mjs";
export function duration(span) {
  const ns = (value) => {
    const m = /^(.*?)(?:\.(\d+))?Z$/.exec(value ?? "");
    if (!m || !Number.isFinite(Date.parse(m[1] + "Z"))) return null;
    return (
      BigInt(Date.parse(m[1] + "Z")) * 1000000n +
      BigInt((m[2] ?? "").padEnd(9, "0").slice(0, 9))
    );
  };
  const start = ns(span.startTime),
    end = ns(span.endTime);
  return start === null || end === null || end < start
    ? null
    : Number(end - start) / 1e6;
}
export function joinTraces(report, traces, truncated = false) {
  const roots = new Map();
  const seen = new Set();
  for (const trace of traces)
    for (const root of trace.spans ?? []) {
      if (
        root.name !== "http_request" ||
        root.labels?.uri !== "/core/v1/lit_action" ||
        root.labels?.["service.name"] !== "lit-api-server"
      )
        continue;
      const id = root.labels?.request_id;
      const identity = `${trace.traceId}/${root.spanId}`;
      if (seen.has(identity)) continue;
      seen.add(identity);
      roots.set(id, [...(roots.get(id) ?? []), { trace, root }]);
    }
  const samples = report.samples.map((sample) => {
    const hits = roots.get(sample.requestId) ?? [];
    if (hits.length !== 1)
      return {
        ...sample,
        telemetryStatus: hits.length ? "ambiguous" : "missing",
      };
    const { root, trace } = hits[0],
      serverMs = duration(root);
    if (serverMs === null || serverMs > sample.clientMs)
      return {
        ...sample,
        telemetryStatus: "invalid-duration",
        traceId: trace.traceId,
      };
    const byId = new Map(trace.spans.map((s) => [s.spanId, s]));
    const belongs = (s) => {
      const visited = new Set();
      while (s && !visited.has(s.spanId)) {
        if (s.spanId === root.spanId) return true;
        visited.add(s.spanId);
        s = byId.get(s.parentSpanId);
      }
      return false;
    };
    const spans = {};
    for (const name of [
      "execute_with_worker",
      "get_private_key",
      "accounts::get_account_wallet_address",
      "accounts::get_billing_wallet_address",
    ]) {
      const found = trace.spans.filter((s) => s.name === name && belongs(s));
      const durations = found.map(duration);
      spans[name] = {
        count: found.length,
        totalMs:
          !found.length || durations.includes(null)
            ? null
            : durations.reduce((a, b) => a + b, 0),
      };
    }
    return {
      ...sample,
      telemetryStatus: "matched",
      traceId: trace.traceId,
      serverMs,
      outsideServerMs: sample.clientMs - serverMs,
      spans,
    };
  });
  const matched = samples.filter((s) => s.telemetryStatus === "matched").length;
  const measurements = Object.fromEntries(
    OPERATIONS.map((op) => {
      const valid = samples.filter(
        (s) =>
          s.operation === op &&
          !s.warmup &&
          s.ok &&
          s.telemetryStatus === "matched",
      );
      return [
        op,
        {
          server: stats(valid.map((s) => s.serverMs)),
          outsideServer: stats(valid.map((s) => s.outsideServerMs)),
        },
      ];
    }),
  );
  return {
    ...report,
    samples,
    telemetry: {
      status:
        !truncated && matched === samples.length ? "complete" : "incomplete",
      truncated,
      matched,
      total: samples.length,
      method:
        "Exact server-generated request ID join. Outside-server = client elapsed minus server HTTP span; includes network, proxy, client and uninstrumented overhead, not pure RTT. Spans overlap; do not sum parent and child times.",
      measurements,
    },
  };
}
