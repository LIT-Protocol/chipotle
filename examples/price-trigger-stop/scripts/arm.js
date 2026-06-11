// Arm the stop: register the price poller as a lit-triggers SCHEDULE trigger.
//
// lit-triggers has no native "price" trigger kind — and that is the point of
// the trust model: the service only invokes the action on a cron heartbeat.
// The PRICE CHECK and every policy fence run inside the attested action,
// against the venue's own API, so the poller cannot lie about the price and
// cannot change the bounds. The bounds (stopPrice/floorPrice/maxAmount/...)
// are frozen into the trigger's default_params here, at arm time, by YOU —
// cron fires carry only {source, scheduled_at, cron} and cannot override them.
//
//   npm run arm       create the schedule trigger (disables any previous one)
//   npm run disarm    PATCH { enabled: false } on the current trigger
//
// Re-arming after editing .env or the action creates a fresh trigger with the
// current code + bounds and disables the old one (orphans are fine for a demo;
// delete them from the lit-triggers dashboard if you like).

const crypto = require("crypto");
const { execSync } = require("child_process");
const env = require("./_env");
const { composeCode } = require("./_lit");
const { buildParams } = require("./_venue");

async function main() {
  env.load();
  const {
    TRIGGERS_BASE = "https://triggers.litprotocol.com",
    LIT_USAGE_API_KEY,
    CRON = "* * * * *",
  } = process.env;

  const disarmOnly = process.argv.includes("--off");

  console.log("Authorizing this machine with lit-triggers...");
  const token = await authorizeAgent(TRIGGERS_BASE);
  env.upsert("LIT_TRIGGERS_AGENT_TOKEN", token);

  if (disarmOnly) {
    if (!process.env.LT_TRIGGER_ID) throw new Error("LT_TRIGGER_ID missing from .env — nothing to disarm.");
    await patchTrigger(TRIGGERS_BASE, token, process.env.LT_TRIGGER_ID, { enabled: false });
    console.log(`✓ Disarmed (trigger ${process.env.LT_TRIGGER_ID} disabled).`);
    return;
  }

  if (!LIT_USAGE_API_KEY) {
    throw new Error("LIT_USAGE_API_KEY missing from .env — run `npm run setup` first.");
  }

  const defaultParams = buildParams(); // validates the stop policy is fully set

  // Be loud about what leaves this machine: default_params (including any
  // venue credentials in them) are stored by the lit-triggers service. Fine
  // for testnet keys; for production, seal credentials (venue-credentials-v1)
  // and pass ciphertext the action decrypts in-TEE — see README.
  if (defaultParams.sandbox === false && (defaultParams.secret || defaultParams.privateKey)) {
    console.log("\n  *** WARNING: VENUE_SANDBOX=false with raw credentials. ***");
    console.log("  arm.js stores default_params (incl. credentials) with the lit-triggers");
    console.log("  service. Use testnet keys here; production keys belong in sealed");
    console.log("  venue-credentials-v1 ciphertext (see README). Ctrl-C now to abort.\n");
    await sleep(5000);
  }
  if (defaultParams.dryRun) {
    console.log("  (DRY_RUN=true — the armed trigger will evaluate fences but never trade.)");
  }

  // One armed stop at a time: disable any previous trigger before creating
  // the replacement, so two crons can't race the same policy.
  if (process.env.LT_TRIGGER_ID) {
    console.log(`Disabling previous trigger ${process.env.LT_TRIGGER_ID}...`);
    await patchTrigger(TRIGGERS_BASE, token, process.env.LT_TRIGGER_ID, { enabled: false }).catch(() => {});
  }

  console.log(`Creating the schedule trigger (cron "${CRON}")...`);
  const trigger = await createTrigger(TRIGGERS_BASE, token, {
    name: "price-trigger-stop",
    kind: "schedule",
    action_code: composeCode(), // lit-venues bundle + action, same bytes setup registered
    default_params: defaultParams,
    usage_api_key: LIT_USAGE_API_KEY,
    max_runs_per_minute: 10,
    max_queued_runs: 5,
    config: { cron: CRON },
  });
  env.upsert("LT_TRIGGER_ID", trigger.id);

  console.log("\n✓ Armed.\n");
  console.log("  Trigger id:", trigger.id);
  console.log("  Cron:      ", CRON);
  console.log(`  Policy:     stop<=${defaultParams.stopPrice} floor>=${defaultParams.floorPrice} amount=${defaultParams.amount} (max ${defaultParams.maxAmount}) on ${defaultParams.venueId}${defaultParams.sandbox ? " sandbox" : ""}`);
  console.log("\nWatch runs (every cron tick appears here, triggered or not):");
  console.log(`  curl -fsS -H "authorization: Bearer $LIT_TRIGGERS_AGENT_TOKEN" \\`);
  console.log(`    "${TRIGGERS_BASE}/api/triggers/${trigger.id}/runs?limit=5"`);
  console.log("\nWhen the stop has fired and sold, DISARM IT: npm run disarm");
  console.log("(a stop is one-shot by intent; the venue-side fences make extra");
  console.log("fires safe, but they will keep showing up as runs).");
}

// --- lit-triggers (same browser-handshake pattern as examples/lit-triggers) --

async function authorizeAgent(base) {
  const existing = process.env.LIT_TRIGGERS_AGENT_TOKEN;
  if (existing && (await meOk(base, existing))) {
    console.log("  reusing existing authorized agent token");
    return existing;
  }
  const token = crypto.randomBytes(36).toString("base64url");
  const challenge = crypto.createHash("sha256").update(token).digest("base64url");
  const url = `${base}/agent/authorize?challenge=${encodeURIComponent(challenge)}`;
  console.log("\n  Opening the authorization page. Sign in if needed, then click");
  console.log('  "Authorize agent". Waiting for approval...\n');
  console.log(`  ${url}\n`);
  openBrowser(url);
  const deadline = Date.now() + 5 * 60 * 1000;
  while (Date.now() < deadline) {
    await sleep(3000);
    if (await meOk(base, token)) return token;
  }
  throw new Error("timed out waiting for agent authorization (5 min)");
}

async function meOk(base, token) {
  try {
    const res = await fetch(`${base}/api/me`, { headers: { authorization: `Bearer ${token}` } });
    return res.ok;
  } catch {
    return false;
  }
}

async function createTrigger(base, token, body) {
  const res = await fetch(`${base}/api/triggers`, {
    method: "POST",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const out = await res.json();
  if (!res.ok) throw new Error(`create trigger -> ${res.status}: ${JSON.stringify(out)}`);
  return out;
}

async function patchTrigger(base, token, triggerId, body) {
  const res = await fetch(`${base}/api/triggers/${triggerId}`, {
    method: "PATCH",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`patch trigger -> ${res.status}`);
}

function openBrowser(url) {
  const cmd = process.platform === "darwin" ? "open" : process.platform === "win32" ? 'start ""' : "xdg-open";
  try { execSync(`${cmd} "${url}"`, { stdio: "ignore" }); } catch {}
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

main().catch((err) => {
  console.error("\nArm failed:", err.message);
  process.exit(1);
});
