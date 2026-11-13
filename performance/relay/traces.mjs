// Optional read-only enrichment. ADC or an explicit local reader key; never stores raw traces.
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { GoogleAuth } from "google-auth-library";
import { joinTraces } from "./trace-results.mjs";
const [input, output] = process.argv.slice(2);
const project = process.env.GCP_PROJECT;
if (!input || !output || !/^[a-z][a-z0-9-]+$/.test(project ?? ""))
  throw new Error(
    "Usage: GCP_PROJECT=chipotle-next node traces.mjs INPUT.json OUTPUT.json (distinct files)",
  );
if (resolve(input) === resolve(output))
  throw new Error(
    "Preserve original benchmark report; choose a different output",
  );
const report = JSON.parse(readFileSync(input));
if (
  report.status !== "passed" ||
  !Array.isArray(report.samples) ||
  report.samples.length !== 60
)
  throw new Error("Expected a complete successful benchmark");
const start = Date.parse(report.startedAt),
  end = Date.parse(report.completedAt);
if (!Number.isFinite(start) || !Number.isFinite(end) || end < start)
  throw new Error("Invalid benchmark window");
const auth = new GoogleAuth({
  keyFilename: process.env.CHIPOTLE_OBSERVABILITY_CREDENTIALS,
  scopes: ["https://www.googleapis.com/auth/cloud-platform"],
});
const traces = [];
let pageToken,
  truncated = false;
try {
  const client = await auth.getClient();
  do {
    const url = new URL(
      `https://cloudtrace.googleapis.com/v1/projects/${project}/traces`,
    );
    url.search = new URLSearchParams({
      startTime: new Date(start - 2000).toISOString(),
      endTime: new Date(end + 2000).toISOString(),
      view: "COMPLETE",
      pageSize: "100",
      filter: "+span:http_request +uri:/core/v1/lit_action",
      ...(pageToken ? { pageToken } : {}),
    });
    const { data } = await client.request({ url: url.href, timeout: 60_000 });
    traces.push(...(data.traces ?? []));
    if (data.nextPageToken && data.nextPageToken === pageToken)
      throw new Error("Repeated page token");
    pageToken = data.nextPageToken;
    if (pageToken && traces.length >= 1000) {
      truncated = true;
      break;
    }
    if (pageToken) await new Promise((r) => setTimeout(r, 6000));
  } while (pageToken);
} catch {
  console.error(
    "GCP trace query failed; check reader authentication, IAM permissions, and quota. No timing report was written.",
  );
  process.exit(1);
}
const result = joinTraces(report, traces, truncated);
writeFileSync(output, JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify(result.telemetry, null, 2));
if (result.telemetry.status !== "complete") process.exitCode = 2;
