// Publishes @lit-protocol/keychain, bumping the version only when the package
// contents actually changed relative to what the registry already serves.
//
//   node scripts/publish-sdk.mjs [--bump patch|minor|major] [--dry-run] [--no-commit]
//
// Decision table (local = sdk/package.json version, published = npm dist-tag latest):
//   local > published            → publish local as-is (someone already bumped)
//   local == published, changed  → bump (default patch), commit + tag, publish
//   local == published, same     → nothing to do
//   local < published            → error; fix the version by hand
// Change detection unpacks the published tarball and a fresh `npm pack` of the
// build and compares every file byte for byte (package.json with the version
// field normalized), so docs-only or metadata-only edits still count as changes.
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const root = path.resolve(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
);
const sdk = path.join(root, "sdk");
const pkgPath = path.join(sdk, "package.json");
const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const bumpLevel = args.includes("--bump")
  ? args[args.indexOf("--bump") + 1]
  : "patch";
if (!["patch", "minor", "major"].includes(bumpLevel))
  throw new Error("--bump must be patch, minor or major");
const dryRun = flag("--dry-run");
const commit = !flag("--no-commit") && !dryRun;
const registry = "https://registry.npmjs.org/";
const log = (message) => process.stderr.write(`publish-sdk: ${message}\n`);

const run = (cmd, cmdArgs, opts = {}) =>
  String(
    execFileSync(cmd, cmdArgs, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "inherit"],
      ...opts,
    }) ?? "",
  ).trim();
const pkg = () => JSON.parse(readFileSync(pkgPath, "utf8"));
const compare = (a, b) => {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) if (pa[i] !== pb[i]) return pa[i] - pb[i];
  return 0;
};
function publishedVersion(name) {
  const result = spawnSync(
    "npm",
    ["view", name, "version", "--registry", registry],
    { encoding: "utf8" },
  );
  if (result.status !== 0) {
    if (/E404|Not found/i.test(result.stderr + result.stdout)) return null;
    throw new Error(`npm view failed: ${result.stderr}`);
  }
  return result.stdout.trim();
}
/** Map of relative file → sha256 for an unpacked tarball, version-normalized. */
function fingerprint(tarball, dir) {
  mkdirSync(dir, { recursive: true });
  run("tar", ["-xzf", tarball, "-C", dir]);
  const base = path.join(dir, "package");
  const files = {};
  const walk = (d) => {
    for (const entry of readdirSync(d)) {
      const full = path.join(d, entry);
      if (statSync(full).isDirectory()) walk(full);
      else {
        const rel = path.relative(base, full);
        let data = readFileSync(full);
        if (rel === "package.json") {
          const json = JSON.parse(data.toString());
          delete json.version;
          data = Buffer.from(JSON.stringify(json));
        }
        files[rel] = createHash("sha256").update(data).digest("hex");
      }
    }
  };
  walk(base);
  return files;
}
/** Downloads the registry tarball directly; `npm pack <name>@<version>` honours
 *  a user's `before=` cooldown setting and would miss recent publishes. */
async function downloadPublished(name, published, dest) {
  const url = run("npm", [
    "view",
    `${name}@${published}`,
    "dist.tarball",
    "--registry",
    registry,
  ]);
  const expected = run("npm", [
    "view",
    `${name}@${published}`,
    "dist.integrity",
    "--registry",
    registry,
  ]);
  if (!url.startsWith("https://"))
    throw new Error(`unexpected tarball URL: ${url}`);
  const response = await fetch(url);
  if (!response.ok)
    throw new Error(`tarball download failed (${response.status})`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const [algorithm, digest] = expected.split("-", 2);
  const actual = createHash(algorithm).update(bytes).digest("base64");
  if (actual !== digest)
    throw new Error("published tarball integrity mismatch");
  writeFileSync(dest, bytes);
}
/** `npm pack --json` output, tolerating any non-JSON lines printed before it. */
function parsePackJson(out) {
  const start = out.indexOf("[");
  if (start < 0) throw new Error(`npm pack --json produced no JSON: ${out}`);
  return JSON.parse(out.slice(start));
}
async function changedFiles(published, name) {
  const work = mkdtempSync(path.join(tmpdir(), "keychain-publish-"));
  try {
    // The build already ran above; --ignore-scripts skips the sdk's prepack
    // hook, whose "Built N catalog actions" log would otherwise land in the
    // stdout we parse as JSON (npm 11 forwards lifecycle output to stdout).
    const localTar = run(
      "npm",
      ["pack", "--pack-destination", work, "--json", "--ignore-scripts"],
      { cwd: sdk },
    );
    const localPath = path.join(work, parsePackJson(localTar)[0].filename);
    const remotePath = path.join(work, "published.tgz");
    await downloadPublished(name, published, remotePath);
    const a = fingerprint(localPath, path.join(work, "local"));
    const b = fingerprint(remotePath, path.join(work, "remote"));
    const names = new Set([...Object.keys(a), ...Object.keys(b)]);
    return [...names].filter((f) => a[f] !== b[f]).sort();
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

const { name, version: local } = pkg();
log(`building ${name}`);
run("npm", ["run", "build"], { cwd: root, stdio: "inherit" });
const published = publishedVersion(name);
log(`local ${local}, published ${published ?? "(none)"}`);
let versionToPublish = local;
if (published !== null) {
  const order = compare(local, published);
  if (order < 0)
    throw new Error(
      `sdk/package.json ${local} is behind the registry (${published}); set a newer version`,
    );
  if (order === 0) {
    const changed = await changedFiles(published, name);
    if (changed.length === 0) {
      log(
        `registry already has these exact contents at ${published}; nothing to publish`,
      );
      process.exit(0);
    }
    log(
      `${changed.length} file(s) differ from ${published}: ${changed.join(", ")}`,
    );
    versionToPublish = run(
      "npm",
      ["version", bumpLevel, "--no-git-tag-version"],
      { cwd: sdk },
    ).replace(/^v/, "");
    log(`bumped ${bumpLevel} → ${versionToPublish}`);
    if (commit) {
      run("git", ["add", path.relative(root, pkgPath)], { cwd: root });
      run(
        "git",
        [
          "commit",
          "-q",
          "-m",
          `keychain: publish @lit-protocol/keychain ${versionToPublish}`,
        ],
        { cwd: root },
      );
      run("git", ["tag", `keychain-sdk-v${versionToPublish}`], { cwd: root });
      log(
        `committed and tagged keychain-sdk-v${versionToPublish}; push the tag with the branch`,
      );
    } else log("version bump left uncommitted (--dry-run/--no-commit)");
  }
}
if (
  !dryRun &&
  spawnSync("npm", ["whoami", "--registry", registry], { encoding: "utf8" })
    .status !== 0
)
  run("npm", ["login", "--scope=@lit-protocol", "--registry", registry], {
    cwd: sdk,
    stdio: "inherit",
  });
log(
  `${dryRun ? "dry-run publishing" : "publishing"} ${name}@${versionToPublish}`,
);
run(
  "npm",
  [
    "publish",
    "--access",
    "public",
    "--registry",
    registry,
    ...(dryRun ? ["--dry-run"] : []),
  ],
  {
    cwd: sdk,
    stdio: "inherit",
  },
);
if (dryRun && versionToPublish !== local) {
  run("git", ["checkout", "--", path.relative(root, pkgPath)], { cwd: root });
  log(`dry run: restored sdk/package.json to ${local}`);
}
