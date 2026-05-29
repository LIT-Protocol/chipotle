// End-to-end client: simulate a signed GitHub `release.published` webhook
// delivery, wait for the trigger run, then read the on-chain registry to
// confirm the attestation landed.
//
// This computes X-Hub-Signature-256 exactly as GitHub does (HMAC-SHA256 over
// the raw body with the shared secret), so it exercises the identical
// verification path. For a GitHub-native test, point a real repo webhook at
// WEBHOOK_URL instead (content type application/json, secret =
// RELEASE_WEBHOOK_SECRET, events = Releases).
//
// Usage:
//   npm run attest                       # defaults below
//   npm run attest -- --tag v1.2.3 --repo owner/name --commitish main

const crypto = require("crypto");
const { ethers } = require("ethers");
const env = require("./_env");

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i !== -1 && process.argv[i + 1] ? process.argv[i + 1] : def;
}

async function main() {
  env.load();
  const {
    WEBHOOK_URL,
    RELEASE_WEBHOOK_SECRET,
    LIT_TRIGGERS_AGENT_TOKEN,
    TRIGGERS_BASE = "https://triggers.litprotocol.com",
    TRIGGER_ID,
    RELEASE_REGISTRY_BASE_SEPOLIA,
    BASE_SEPOLIA_RPC_URL = "https://sepolia.base.org",
  } = process.env;

  for (const k of ["WEBHOOK_URL", "RELEASE_WEBHOOK_SECRET", "RELEASE_REGISTRY_BASE_SEPOLIA"]) {
    if (!process.env[k]) throw new Error(`${k} missing from .env — run \`npm run setup\` first.`);
  }

  const repo = arg("repo", "LIT-Protocol/chipotle");
  const tag = arg("tag", "v0.0.1-demo");
  const commitish = arg("commitish", "main");

  const payload = {
    action: "published",
    release: { tag_name: tag, target_commitish: commitish },
    repository: { full_name: repo },
  };
  const raw = JSON.stringify(payload);
  const sig =
    "sha256=" + crypto.createHmac("sha256", RELEASE_WEBHOOK_SECRET).update(raw).digest("hex");

  console.log(`Firing signed release webhook: ${repo} ${tag} @ ${commitish}`);
  const res = await fetch(WEBHOOK_URL, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-hub-signature-256": sig,
      "x-github-event": "release",
    },
    body: raw,
  });
  const queued = await res.json();
  console.log("  queued:", JSON.stringify(queued));

  if (LIT_TRIGGERS_AGENT_TOKEN && TRIGGER_ID) {
    console.log("Waiting for the trigger run...");
    const run = await waitForRun(TRIGGERS_BASE, LIT_TRIGGERS_AGENT_TOKEN, TRIGGER_ID, queued.run_id);
    const inner = run && run.response && run.response.response;
    console.log("  run status:", run && run.status);
    console.log("  action result:", JSON.stringify(inner));
  }

  console.log("Reading ReleaseRegistry on-chain...");
  const provider = new ethers.providers.JsonRpcProvider(BASE_SEPOLIA_RPC_URL);
  const registry = new ethers.Contract(
    RELEASE_REGISTRY_BASE_SEPOLIA,
    ["function getRelease(string,string) view returns (string commitish, uint256 timestamp)"],
    provider
  );
  const [onchainCommitish, timestamp] = await registry.getRelease(repo, tag);
  console.log(`  getRelease(${repo}, ${tag}) ->`);
  console.log(`    commitish: ${onchainCommitish}`);
  console.log(`    timestamp: ${timestamp.toString()}`);

  if (onchainCommitish === commitish && timestamp.gt(0)) {
    console.log("\n✓ Attestation recorded on-chain by the keyless action wallet.");
  } else {
    console.log("\n… not recorded yet (the run may still be processing). Re-run to re-check.");
  }
}

async function waitForRun(base, token, triggerId, runId) {
  for (let i = 0; i < 30; i++) {
    await new Promise((r) => setTimeout(r, 3000));
    const res = await fetch(`${base}/api/triggers/${triggerId}/runs?limit=5`, {
      headers: { authorization: `Bearer ${token}` },
    });
    if (!res.ok) continue;
    const { runs } = await res.json();
    const run = runs.find((r) => r.id === runId) || runs[0];
    if (run && (run.status === "success" || run.status === "failed")) return run;
  }
  return null;
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
