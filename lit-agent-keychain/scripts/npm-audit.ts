// CI's npm audit: `npm audit --audit-level=low`, except for the advisories
// that npm-audit-allowlist.json accepts at specific install paths. The rule is
// in scripts/npm-audit-policy.ts.
//
//   npx tsx scripts/npm-audit.ts
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
  evaluateAudit,
  npmExitCode,
  oneLine,
  type Allowlist,
} from "./npm-audit-policy.ts";

const root = path.resolve(import.meta.dirname, "..");
const allowlist: Allowlist = JSON.parse(
  readFileSync(path.join(root, "npm-audit-allowlist.json"), "utf8"),
);
// Annotation text: GitHub Actions decodes these escapes inside the message.
const data = (s: string) =>
  s.replace(/%/g, "%25").replace(/\r/g, "%0D").replace(/\n/g, "%0A");
const run = spawnSync("npm", ["audit", "--json", "--audit-level=low"], {
  cwd: root,
  encoding: "utf8",
  maxBuffer: 256 * 1024 * 1024,
});
if (run.error) {
  console.error(
    `npm-audit: could not run npm audit: ${oneLine(run.error.message)}`,
  );
  process.exit(1);
}
let report;
try {
  report = JSON.parse(run.stdout);
} catch {
  console.error(
    `npm-audit: npm audit (${run.signal ? `signal ${run.signal}` : `exit ${run.status}`}) printed no JSON report:`,
  );
  // Prefixed so npm's output cannot start a workflow command.
  for (const line of `${run.stdout}${run.stderr}`.slice(0, 8000).split("\n"))
    console.error(`npm: ${oneLine(line)}`);
  process.exit(1);
}
const verdict = evaluateAudit(report, allowlist);
// npm exits 1 when its summary has anything at low or above, and 0 otherwise.
// Any other ending means the run failed, whatever it printed.
const expected = npmExitCode(report);
if (run.signal || run.status !== expected)
  verdict.failures.push(
    `npm audit ended with ${run.signal ? `signal ${run.signal}` : `exit ${run.status}`}, but its report implies exit ${expected}`,
  );
for (const line of verdict.accepted) console.log(`npm-audit: accepted ${line}`);
if (verdict.dependents.length)
  console.log(
    `npm-audit: flagged only through a dependency: ${verdict.dependents.join(", ")}`,
  );
if (verdict.accepted.length)
  console.log(
    `::warning title=npm audit::${verdict.accepted.length} exception(s) from npm-audit-allowlist.json applied; see the step log`,
  );
for (const line of verdict.unused)
  console.log(`::warning title=npm audit allowlist::${data(line)}`);
if (verdict.failures.length) {
  for (const line of verdict.failures)
    console.log(`::error title=npm audit::${data(line)}`);
  console.log(
    `npm-audit: ${verdict.failures.length} finding(s) not accepted; \`npm audit\` shows the details`,
  );
  process.exit(1);
}
console.log("npm-audit: ok");
