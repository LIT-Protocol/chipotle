// Pins @lit-protocol/agent-keychain-library to an exact commit of the public repo.
//
//   npm run library:pin <40-hex commit sha>
//
// The dependency is a codeload tarball URL rather than a git spec so `npm ci` in
// CI and Docker needs neither git nor GitHub credentials, and the lockfile carries
// a sha512 integrity for the exact archive. After pinning, rebuild with
// `npm run build:actions -- --update-lock` and check that the lock diff only adds
// hashes for new actions; an existing hash changing means shared bytes moved.
import { execFileSync } from "node:child_process";
const sha = process.argv[2];
if (!/^[0-9a-f]{40}$/.test(sha ?? "")) {
  console.error("Usage: npm run library:pin <40-hex commit sha>");
  process.exit(2);
}
const url = `https://codeload.github.com/LIT-Protocol/agent-keychain-library/tar.gz/${sha}`;
execFileSync(
  "npm",
  ["install", "--save-exact", "--no-audit", "--no-fund", url],
  {
    stdio: "inherit",
  },
);
console.log(
  `Pinned @lit-protocol/agent-keychain-library to ${sha}. Now run: npm run build:actions -- --update-lock`,
);
