#!/usr/bin/env node

import fs from "node:fs";

const [summaryPath = "soak-summary.json", testName = "loadtest"] =
  process.argv.slice(2);
const outputPath = process.env.GITHUB_STEP_SUMMARY;

function appendMarkdown(markdown) {
  if (outputPath) {
    fs.appendFileSync(outputPath, markdown);
    return;
  }
  process.stdout.write(markdown);
}

function formatMs(value) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "n/a";
  }
  return `${Number(value.toFixed(2))}ms`;
}

function formatPercent(value) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "n/a";
  }
  return `${Number((value * 100).toFixed(2))}%`;
}

function inlineCode(value) {
  if (value === null || value === undefined || value === "") {
    return "`n/a`";
  }
  return `\`${String(value).replaceAll("`", "'")}\``;
}

function scenarioLabel(name) {
  return `\`${name.replaceAll("`", "'")}\``;
}

if (!fs.existsSync(summaryPath)) {
  appendMarkdown(
    [
      "## k6 Load Test Summary",
      "",
      `**Test:** ${inlineCode(testName)}`,
      "",
      `${inlineCode(summaryPath)} was not produced, so no soak latency table is available. Check the k6 log artifact for the full run output.`,
      "",
    ].join("\n"),
  );
  process.exit(0);
}

let summary;
try {
  summary = JSON.parse(fs.readFileSync(summaryPath, "utf8"));
} catch (error) {
  appendMarkdown(
    [
      "## k6 Load Test Summary",
      "",
      `**Test:** ${inlineCode(testName)}`,
      "",
      `Could not parse ${inlineCode(summaryPath)}: ${error.message}`,
      "",
    ].join("\n"),
  );
  process.exit(0);
}

const scenarios = Object.entries(summary.scenarios ?? {}).filter(
  ([, values]) => values && typeof values === "object",
);

const lines = [
  "## k6 Load Test Summary",
  "",
  `**Test:** ${inlineCode(testName)}`,
  `**Base URL:** ${inlineCode(summary.base_url)}`,
  `**Correlation ID:** ${inlineCode(summary.correlation_id)}`,
  "",
];

if (scenarios.length > 0) {
  lines.push(
    "| Scenario | avg | min | med | max | p95 | p99 |",
    "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
  );

  for (const [name, values] of scenarios) {
    lines.push(
      `| ${scenarioLabel(name)} | ${formatMs(values.avg)} | ${formatMs(
        values.min,
      )} | ${formatMs(values.p50)} | ${formatMs(values.max)} | ${formatMs(
        values.p95,
      )} | ${formatMs(values.p99)} |`,
    );
  }
} else {
  lines.push("No per-scenario soak latency metrics were captured.");
}

lines.push(
  "",
  "| Run metric | Value |",
  "| --- | ---: |",
  `| checks rate | ${formatPercent(summary.checks_rate)} |`,
  `| HTTP request failure rate | ${formatPercent(summary.http_req_failed_rate)} |`,
  "",
);

appendMarkdown(lines.join("\n"));
