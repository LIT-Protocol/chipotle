import { test } from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import {
  checkAllowlist,
  evaluateAudit,
  npmExitCode,
  type Advisory,
  type Allowlist,
  type AuditReport,
  type Vulnerability,
} from "../scripts/npm-audit-policy.ts";

const root = path.resolve(import.meta.dirname, "..");
const allowlist: Allowlist = JSON.parse(
  await readFile(path.join(root, "npm-audit-allowlist.json"), "utf8"),
);

const advisory = (over: Partial<Advisory> = {}): Advisory => ({
  source: 1243066,
  name: "zod",
  title:
    "Zod can exhaust memory while validating oversized constrained collections",
  url: "https://github.com/advisories/GHSA-mrhx-c7xc-m3w5",
  severity: "high",
  range: "<4.6.6",
  ...over,
});
const WALLET_COPIES = [
  "node_modules/@coinbase/cdp-sdk/node_modules/zod",
  "node_modules/@reown/appkit-controllers/node_modules/zod",
  "node_modules/@reown/appkit-utils/node_modules/zod",
  "node_modules/@reown/appkit-wallet/node_modules/zod",
  "node_modules/@reown/appkit/node_modules/zod",
  "node_modules/@walletconnect/utils/node_modules/zod",
];
const TEMPLATE_COPY = "node_modules/zod-action-templates";
const vulnerability = (
  name: string,
  via: Vulnerability["via"],
  nodes = [`node_modules/${name}`],
  severity = "high",
): Vulnerability => ({ name, severity, via, nodes });
// A report as npm writes it: the summary counts each flagged package once, by
// its severity, and npm's exit code comes from the summary.
function audit(vulnerabilities: Record<string, Vulnerability>): AuditReport {
  const summary: Record<string, number> = {
    info: 0,
    low: 0,
    moderate: 0,
    high: 0,
    critical: 0,
  };
  for (const v of Object.values(vulnerabilities))
    summary[v.severity] = (summary[v.severity] ?? 0) + 1;
  summary.total = Object.keys(vulnerabilities).length;
  return {
    auditReportVersion: 2,
    vulnerabilities,
    metadata: { vulnerabilities: summary },
  };
}
// The shape of this branch's real `npm audit --json`, trimmed: zod carries the
// advisory and the wallet packages are flagged only through it.
function report(
  zodNodes = [...WALLET_COPIES, TEMPLATE_COPY],
  extra: Record<string, Vulnerability> = {},
): AuditReport {
  return audit({
    zod: vulnerability("zod", [advisory()], zodNodes),
    "@reown/appkit-wallet": vulnerability("@reown/appkit-wallet", ["zod"]),
    "@wagmi/connectors": vulnerability("@wagmi/connectors", [
      "@reown/appkit-wallet",
    ]),
    wagmi: vulnerability("wagmi", ["@wagmi/connectors"]),
    ...extra,
  });
}

test("the allowlist accepts the zod advisory only at the listed copies", () => {
  const verdict = evaluateAudit(report(), allowlist);
  assert.deepEqual(verdict.failures, []);
  assert.deepEqual(verdict.unused, []);
  assert.equal(verdict.accepted.length, 2);
  assert.match(verdict.accepted[0], /GHSA-mrhx-c7xc-m3w5 .*appkit-wallet/);
  assert.match(verdict.accepted[1], /zod-action-templates: zod 4\.3\.6/);
  assert.deepEqual(verdict.dependents, [
    "@reown/appkit-wallet",
    "@wagmi/connectors",
    "wagmi",
  ]);
});

test("our own zod or a new copy below the fix fails", () => {
  for (const extra of [
    "node_modules/zod",
    "node_modules/x402/node_modules/zod",
  ]) {
    const verdict = evaluateAudit(
      report([...WALLET_COPIES, TEMPLATE_COPY, extra]),
      allowlist,
    );
    assert.equal(verdict.failures.length, 1, extra);
    assert.match(
      verdict.failures[0],
      new RegExp(`Not accepted at ${extra}\\.`),
    );
    assert.equal(verdict.accepted.length, 0, extra);
  }
});

test("another advisory, or an advisory on another package, fails", () => {
  const other = advisory({
    source: 1,
    title: "Another zod issue",
    url: "https://github.com/advisories/GHSA-2222-3333-4444",
    severity: "moderate",
  });
  const twoAdvisories = report();
  twoAdvisories.vulnerabilities!.zod.via.push(other);
  let verdict = evaluateAudit(twoAdvisories, allowlist);
  assert.equal(verdict.failures.length, 1);
  assert.match(verdict.failures[0], /^zod GHSA-2222-3333-4444 \(moderate/);

  verdict = evaluateAudit(
    report(undefined, {
      viem: vulnerability("viem", [
        "abitype",
        advisory({
          name: "viem",
          url: "https://github.com/advisories/GHSA-5555-6666-7777",
        }),
      ]),
      abitype: vulnerability("abitype", ["zod"]),
    }),
    allowlist,
  );
  assert.equal(verdict.failures.length, 1);
  assert.match(verdict.failures[0], /^viem GHSA-5555-6666-7777 \(high/);
});

test("severity threshold matches --audit-level=low", () => {
  const jose = (severity: string) =>
    vulnerability(
      "jose",
      [
        advisory({
          name: "jose",
          severity,
          url: "https://github.com/advisories/GHSA-7777-8888-9999",
        }),
      ],
      undefined,
      severity,
    );
  assert.deepEqual(
    evaluateAudit(report(undefined, { jose: jose("info") }), allowlist)
      .failures,
    [],
  );
  for (const severity of ["low", "critical", "unrated"]) {
    const verdict = evaluateAudit(
      report(undefined, { jose: jose(severity) }),
      allowlist,
    );
    assert.equal(verdict.failures.length, 1, severity);
  }
  // npm's own exit code for the same reports.
  assert.equal(npmExitCode(audit({})), 0);
  assert.equal(npmExitCode(audit({ jose: jose("info") })), 0);
  assert.equal(npmExitCode(audit({ jose: jose("low") })), 1);
  assert.equal(npmExitCode(report()), 1);
});

test("a missing, failed or inconsistent report fails", () => {
  for (const bad of [
    { error: { code: "ENOTFOUND", summary: "request to registry failed" } },
    { ...audit({}), error: { code: "EAUDITNOPJSON" } },
    { auditReportVersion: 2, metadata: audit({}).metadata },
    { auditReportVersion: 2, vulnerabilities: {} },
    {},
    { ...audit({}), auditReportVersion: 1 },
    null,
  ]) {
    const verdict = evaluateAudit(bad as AuditReport, allowlist);
    assert.equal(verdict.failures.length, 1, JSON.stringify(bad));
    assert.match(verdict.failures[0], /did not return a report/);
  }
  const ghost = report(undefined, {
    wagmi: vulnerability("wagmi", ["@wagmi/ghost", "constructor"]),
  });
  assert.match(
    evaluateAudit(ghost, allowlist).failures.join("\n"),
    /flagged through @wagmi\/ghost, which the report does not list\nwagmi: flagged through constructor, which/,
  );
  const noNodes = report([]);
  assert.match(
    evaluateAudit(noNodes, allowlist).failures.join("\n"),
    /Not accepted at an unknown path/,
  );
});

test("every flagged package needs an advisory behind it, at its severity", () => {
  const p = (via: unknown, severity = "high", name = "p") =>
    ({ name, severity, via, nodes: [`node_modules/${name}`] }) as Vulnerability;
  for (const via of [[], undefined, ["p"]])
    assert.deepEqual(
      evaluateAudit(audit({ p: p(via) }), allowlist).failures,
      ["p: flagged high with no advisory behind it"],
      JSON.stringify(via),
    );
  assert.deepEqual(
    evaluateAudit(audit({ p: p(["q"]), q: p(["p"], "high", "q") }), allowlist)
      .failures,
    [
      "p: flagged high with no advisory behind it",
      "q: flagged high with no advisory behind it",
    ],
  );
  assert.deepEqual(evaluateAudit(audit({ p: p([42]) }), allowlist).failures, [
    "p: flagged high with no advisory behind it",
    "p: unreadable `via` entry",
  ]);
  // npm doesn't fail on info, so neither does this.
  assert.deepEqual(
    evaluateAudit(audit({ p: p([], "info") }), allowlist).failures,
    [],
  );
  assert.deepEqual(
    evaluateAudit(
      report(undefined, {
        wagmi: vulnerability(
          "wagmi",
          ["@wagmi/connectors"],
          undefined,
          "critical",
        ),
      }),
      allowlist,
    ).failures,
    ["wagmi: flagged critical, above every advisory behind it"],
  );
});

test("the summary that sets npm's exit code must match the findings", () => {
  const fewer = report();
  fewer.metadata!.vulnerabilities!.high -= 1;
  assert.deepEqual(evaluateAudit(fewer, allowlist).failures, [
    "npm audit's summary says high: 3, but it lists 4",
  ]);
  const more = report();
  more.metadata!.vulnerabilities!.critical = 1;
  more.metadata!.vulnerabilities!.total += 1;
  assert.deepEqual(evaluateAudit(more, allowlist).failures, [
    "npm audit's summary says critical: 1, but it lists 0",
    "npm audit's summary says total: 5, but it lists 4",
  ]);
});

test("an empty allowlist is plain --audit-level=low; stale entries are reported", () => {
  assert.equal(evaluateAudit(report(), { accepted: [] }).failures.length, 1);
  assert.deepEqual(evaluateAudit(audit({}), allowlist).failures, []);
  const verdict = evaluateAudit(report(WALLET_COPIES), allowlist);
  assert.deepEqual(verdict.failures, []);
  assert.deepEqual(verdict.unused, [
    `GHSA-mrhx-c7xc-m3w5 for zod no longer matches ${TEMPLATE_COPY}; remove it from npm-audit-allowlist.json`,
  ]);
});

test("allowlist entries need a GHSA id, install paths and a reason", () => {
  const good = allowlist.accepted[0];
  checkAllowlist(allowlist);
  for (const bad of [
    { ...good, advisory: "CVE-2023-54404" },
    { ...good, package: "" },
    { ...good, paths: [] },
    { ...good, paths: ["@reown/appkit-wallet/node_modules/zod"] },
    { ...good, reason: " " },
  ])
    assert.throws(() => checkAllowlist({ accepted: [bad] }));
  assert.throws(() => checkAllowlist({} as Allowlist));
});

type Run = (
  out: string,
  ending?: number | "TERM",
) => Promise<{ code: number; out: string }>;
// Runs scripts/npm-audit.ts with a stub `npm` that prints `out` and then
// exits with `ending`, or kills itself with SIGTERM.
async function withStubNpm(fn: (run: Run) => Promise<void>) {
  const dir = await mkdtemp(path.join(tmpdir(), "npm-audit-"));
  try {
    const stub = path.join(dir, "npm");
    await writeFile(
      stub,
      `#!/bin/sh\n[ "$*" = "audit --json --audit-level=low" ] || exit 9\n` +
        `cat "${dir}/out"\nending=$(cat "${dir}/ending")\n` +
        `[ "$ending" = TERM ] && kill -TERM $$\nexit "$ending"\n`,
    );
    await chmod(stub, 0o755);
    await fn(async (out, ending = 1) => {
      await writeFile(path.join(dir, "out"), out);
      await writeFile(path.join(dir, "ending"), String(ending));
      try {
        const { stdout, stderr } = await promisify(execFile)(
          process.execPath,
          ["--import", "tsx", "scripts/npm-audit.ts"],
          {
            cwd: root,
            env: {
              ...process.env,
              PATH: `${dir}${path.delimiter}${process.env.PATH}`,
            },
          },
        );
        return { code: 0, out: `${stdout}${stderr}` };
      } catch (e: any) {
        return { code: e.code as number, out: `${e.stdout}${e.stderr}` };
      }
    });
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
}

test("CLI exit codes, with a stub npm on PATH", async () => {
  await withStubNpm(async (run) => {
    let r = await run(JSON.stringify(report()));
    assert.equal(r.code, 0, r.out);
    assert.match(r.out, /::warning title=npm audit::2 exception\(s\)/);
    assert.match(r.out, /npm-audit: ok/);

    r = await run(JSON.stringify(audit({})), 0);
    assert.equal(r.code, 0, r.out);
    assert.match(r.out, /npm-audit: ok/);

    r = await run(
      JSON.stringify(report([...WALLET_COPIES, "node_modules/zod"])),
    );
    assert.equal(r.code, 1, r.out);
    assert.match(r.out, /::error title=npm audit::zod GHSA-mrhx-c7xc-m3w5/);
    assert.match(
      r.out,
      /::warning title=npm audit allowlist::.*zod-action-templates/,
    );

    r = await run("npm error code ENOTFOUND\n");
    assert.equal(r.code, 1, r.out);
    assert.match(r.out, /printed no JSON report/);
  });
});

test("CLI fails when npm ends other than its report implies", async () => {
  await withStubNpm(async (run) => {
    let r = await run(JSON.stringify(audit({})), 2);
    assert.equal(r.code, 1, r.out);
    assert.match(r.out, /ended with exit 2, but its report implies exit 0/);

    r = await run(JSON.stringify(report()), 0);
    assert.equal(r.code, 1, r.out);
    assert.match(r.out, /ended with exit 0, but its report implies exit 1/);

    r = await run(JSON.stringify(report()), "TERM");
    assert.equal(r.code, 1, r.out);
    assert.match(
      r.out,
      /ended with signal SIGTERM, but its report implies exit 1/,
    );
  });
});

test("report text cannot start a workflow command", async () => {
  const lines = (out: string) => out.split(/\r\n|\r|\n/);
  await withStubNpm(async (run) => {
    const name = "evil\n::error::name";
    const title = "bad\n::warning title=FORGED::injected\r::error::x %0A";
    let r = await run(
      JSON.stringify(
        report(undefined, {
          [name]: vulnerability(name, [
            advisory({
              name,
              title,
              url: "https://github.com/advisories/GHSA-2222-3333-4444",
            }),
          ]),
        }),
      ),
    );
    assert.equal(r.code, 1, r.out);
    for (const line of lines(r.out).filter((l) => l.startsWith("::")))
      assert.match(line, /^::(warning|error) title=npm audit( allowlist)?::/);
    assert.match(
      r.out,
      /^::error title=npm audit::evil ::error::name GHSA-2222-3333-4444 \(high, <4\.6\.6\): bad ::warning title=FORGED::injected ::error::x %250A\./m,
    );

    r = await run("::error::forged\nnot json\n");
    assert.equal(r.code, 1, r.out);
    assert.deepEqual(
      lines(r.out).filter((l) => l.startsWith("::")),
      [],
    );
    assert.match(r.out, /^npm: ::error::forged$/m);
  });
});
