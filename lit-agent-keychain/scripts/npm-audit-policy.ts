// Decides whether an `npm audit --json` report (auditReportVersion 2) passes,
// given the exceptions in npm-audit-allowlist.json.
//
// npm lists a package as vulnerable either because an advisory names it (an
// advisory object in `via`) or only because one of its dependencies is
// vulnerable (package names in `via`). The report passes when every advisory
// of severity low or above is accepted for that package, and every vulnerable
// install path of the package (`nodes`) is one of the accepted paths. A
// package flagged only through its dependencies passes or fails with them,
// but some advisory behind it must explain its severity. The summary
// (`metadata.vulnerabilities`), which sets npm's own exit code, must agree
// with the findings. With an empty allowlist this is `npm audit
// --audit-level=low`.

export type Advisory = {
  source: number;
  name: string;
  title: string;
  url: string;
  severity: string;
  range: string;
};

export type Vulnerability = {
  name: string;
  severity: string;
  via: (string | Advisory)[];
  nodes: string[];
};

export type AuditReport = {
  auditReportVersion?: number;
  vulnerabilities?: Record<string, Vulnerability>;
  metadata?: { vulnerabilities?: Record<string, number> };
  error?: unknown;
};

export type Exception = {
  advisory: string;
  package: string;
  paths: string[];
  reason: string;
};

export type Allowlist = { accepted: Exception[] };

export type Verdict = {
  /** Findings that fail the audit. */
  failures: string[];
  /** Advisories the allowlist accepts, one line each. */
  accepted: string[];
  /** Packages flagged only through a dependency's advisory. */
  dependents: string[];
  /** Allowlisted paths that no advisory matched: the exception can shrink. */
  unused: string[];
};

const SEVERITIES = ["info", "low", "moderate", "high", "critical"];
// An unknown severity ranks above critical, so it always counts.
const rank = (severity: unknown) => {
  const i = SEVERITIES.indexOf(severity as string);
  return i < 0 ? SEVERITIES.length : i;
};

/** Report text on one line, so it cannot start a GitHub Actions command. */
export const oneLine = (value: unknown) =>
  String(value).replace(
    /[\u0000-\u0008\u000a-\u001f\u007f\u2028\u2029]+/g,
    " ",
  );

/** The exit code `npm audit --audit-level=low` derives from this report. */
export function npmExitCode(report: AuditReport): number {
  const summary = report?.metadata?.vulnerabilities ?? {};
  return Object.entries(summary).some(
    ([severity, count]) =>
      count > 0 && SEVERITIES.includes(severity) && rank(severity) >= 1,
  )
    ? 1
    : 0;
}

export function checkAllowlist(allowlist: Allowlist): void {
  if (!Array.isArray(allowlist?.accepted))
    throw new Error("npm-audit-allowlist.json: `accepted` must be an array");
  for (const [i, e] of allowlist.accepted.entries()) {
    const where = `npm-audit-allowlist.json accepted[${i}]`;
    if (!/^GHSA(-[23456789cfghjmpqrvwx]{4}){3}$/.test(e?.advisory))
      throw new Error(`${where}: advisory must be a GHSA id`);
    if (typeof e.package !== "string" || !e.package)
      throw new Error(`${where}: package is required`);
    // Install paths as npm reports them in `nodes`. A path that isn't one
    // never matches, and shows up as unused.
    if (
      !Array.isArray(e.paths) ||
      !e.paths.length ||
      !e.paths.every((p) => typeof p === "string" && /^node_modules\/./.test(p))
    )
      throw new Error(`${where}: paths must be node_modules/... install paths`);
    if (typeof e.reason !== "string" || !e.reason.trim())
      throw new Error(`${where}: reason is required`);
  }
}

const advisoryId = (a: Advisory) =>
  /\/(GHSA(?:-[a-z0-9]{4}){3})$/i.exec(a.url ?? "")?.[1] ??
  `npm-${oneLine(a.source)}`;

export function evaluateAudit(
  report: AuditReport,
  allowlist: Allowlist,
): Verdict {
  checkAllowlist(allowlist);
  const verdict: Verdict = {
    failures: [],
    accepted: [],
    dependents: [],
    unused: [],
  };
  const vulnerabilities = report?.vulnerabilities;
  const summary = report?.metadata?.vulnerabilities;
  if (
    report?.error ||
    report?.auditReportVersion !== 2 ||
    !vulnerabilities ||
    typeof vulnerabilities !== "object" ||
    !summary ||
    typeof summary !== "object"
  ) {
    verdict.failures.push(
      `npm audit did not return a report: ${JSON.stringify(report?.error ?? report).slice(0, 400)}`,
    );
    return verdict;
  }
  const entries = Object.entries(vulnerabilities);
  // npm's exit code comes from the summary, so it must count the findings.
  const counted: Record<string, number> = { total: entries.length };
  for (const [, v] of entries)
    counted[v?.severity] = (counted[v?.severity] ?? 0) + 1;
  for (const key of new Set([...Object.keys(summary), ...Object.keys(counted)]))
    if ((summary[key] ?? 0) !== (counted[key] ?? 0))
      verdict.failures.push(
        `npm audit's summary says ${oneLine(key)}: ${oneLine(summary[key] ?? 0)}, but it lists ${counted[key] ?? 0}`,
      );
  // The advisories behind a package, following package names through `via`.
  const behind = (start: string) => {
    const seen = new Set<string>();
    const found: Advisory[] = [];
    const queue = [start];
    while (queue.length) {
      const name = queue.pop()!;
      if (seen.has(name) || !Object.hasOwn(vulnerabilities, name)) continue;
      seen.add(name);
      const vias = vulnerabilities[name]?.via;
      for (const via of Array.isArray(vias) ? vias : [])
        if (typeof via === "string") queue.push(via);
        else if (via && typeof via === "object") found.push(via);
    }
    return found;
  };
  const used = new Set<string>();
  for (const [name, vulnerability] of entries) {
    const label = oneLine(name);
    const severity = oneLine(vulnerability?.severity);
    const nodes = (
      Array.isArray(vulnerability?.nodes) ? vulnerability.nodes : []
    ).map(oneLine);
    const vias = Array.isArray(vulnerability?.via) ? vulnerability.via : [];
    if (vulnerability?.severity !== "info") {
      const advisories = behind(name);
      if (!advisories.length)
        verdict.failures.push(
          `${label}: flagged ${severity} with no advisory behind it`,
        );
      else if (
        rank(vulnerability?.severity) >
        Math.max(...advisories.map((a) => rank(a.severity)))
      )
        verdict.failures.push(
          `${label}: flagged ${severity}, above every advisory behind it`,
        );
    }
    let ownAdvisory = false;
    for (const via of vias) {
      if (typeof via === "string") {
        if (!Object.hasOwn(vulnerabilities, via))
          verdict.failures.push(
            `${label}: flagged through ${oneLine(via)}, which the report does not list`,
          );
        continue;
      }
      if (!via || typeof via !== "object") {
        verdict.failures.push(`${label}: unreadable \`via\` entry`);
        continue;
      }
      ownAdvisory = true;
      // Same threshold as --audit-level=low; an unknown severity counts.
      if (via.severity === "info") continue;
      const id = advisoryId(via);
      const exceptions = allowlist.accepted
        .map((e, i) => ({ e, i }))
        .filter(({ e }) => e.advisory === id && e.package === name);
      const uncovered = nodes.filter(
        (p) => !exceptions.some(({ e }) => e.paths.includes(p)),
      );
      if (!exceptions.length || uncovered.length || !nodes.length) {
        verdict.failures.push(
          `${label} ${id} (${oneLine(via.severity)}, ${oneLine(via.range)}): ${oneLine(via.title)}. ` +
            `Not accepted at ${(uncovered.length ? uncovered : nodes).join(", ") || "an unknown path"}. ${oneLine(via.url)}`,
        );
        continue;
      }
      for (const { e, i } of exceptions) {
        const matched = nodes.filter((p) => e.paths.includes(p));
        for (const p of matched) used.add(`${i}\n${p}`);
        if (matched.length)
          verdict.accepted.push(
            `${label} ${id} (${oneLine(via.severity)}) at ${matched.join(", ")}: ${oneLine(e.reason)}`,
          );
      }
    }
    if (!ownAdvisory) verdict.dependents.push(label);
  }
  for (const [i, e] of allowlist.accepted.entries())
    for (const p of e.paths)
      if (!used.has(`${i}\n${p}`))
        verdict.unused.push(
          `${e.advisory} for ${oneLine(e.package)} no longer matches ${oneLine(p)}; remove it from npm-audit-allowlist.json`,
        );
  return verdict;
}
