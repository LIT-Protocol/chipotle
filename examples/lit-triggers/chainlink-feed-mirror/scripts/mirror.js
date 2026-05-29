// End-to-end client for the feed mirror.
//
//   npm run mirror -- --simulate    (default-friendly: deterministic + fast)
//     Spins up a TEMPORARY webhook trigger running the SAME action, feeds it a
//     synthetic AnswerUpdated payload, confirms PriceConsumer was written, then
//     deletes the temp trigger. Proves the relay logic without waiting for a
//     real on-chain Chainlink update.
//
//   npm run mirror                  (the real thing)
//     Watches the chain-event trigger created by setup and prints the next
//     real AnswerUpdated relay. May take minutes — Chainlink only emits on a
//     price deviation or its heartbeat.

const { ethers } = require("ethers");
const env = require("./_env");

const SIMULATE = process.argv.includes("--simulate");

async function main() {
  env.load();
  const {
    TRIGGERS_BASE = "https://triggers.litprotocol.com",
    LIT_TRIGGERS_AGENT_TOKEN: TOKEN,
    LIT_USAGE_API_KEY: USAGE,
    TRIGGER_ID,
    PRICE_CONSUMER_BASE_SEPOLIA,
    BASE_SEPOLIA_RPC_URL = "https://sepolia.base.org",
    DEST_CHAIN_ID = "84532",
  } = process.env;

  for (const k of ["LIT_TRIGGERS_AGENT_TOKEN", "PRICE_CONSUMER_BASE_SEPOLIA"]) {
    if (!process.env[k]) throw new Error(`${k} missing from .env — run \`npm run setup\` first.`);
  }

  const provider = new ethers.providers.JsonRpcProvider(BASE_SEPOLIA_RPC_URL);
  const consumer = new ethers.Contract(
    PRICE_CONSUMER_BASE_SEPOLIA,
    [
      "function latestAnswer() view returns (int256)",
      "function roundId() view returns (uint256)",
      "function updatedAt() view returns (uint256)",
    ],
    provider
  );
  const before = await consumer.roundId();
  console.log(`PriceConsumer ${PRICE_CONSUMER_BASE_SEPOLIA} — current roundId ${before.toString()}`);

  let run;
  if (SIMULATE) {
    run = await simulate(TRIGGERS_BASE, TOKEN, USAGE, {
      destRpcUrl: BASE_SEPOLIA_RPC_URL,
      destChainId: DEST_CHAIN_ID,
      consumer: PRICE_CONSUMER_BASE_SEPOLIA,
      gasLimit: "150000",
      dryRun: false,
    }, before);
  } else {
    if (!TRIGGER_ID) throw new Error("TRIGGER_ID missing — run setup, or use --simulate.");
    console.log("Watching the chain-event trigger for the next real AnswerUpdated...");
    run = await waitForFreshRun(TRIGGERS_BASE, TOKEN, TRIGGER_ID);
  }

  const inner = run && run.response && run.response.response;
  console.log(`  run status: ${run && run.status}`);
  console.log(`  action result: ${JSON.stringify(inner)}`);

  const after = await consumer.roundId();
  const answer = await consumer.latestAnswer();
  console.log(`PriceConsumer now — roundId ${after.toString()}, latestAnswer ${answer.toString()}`);
  if (after.gt(before)) {
    console.log("\n✓ Chainlink price relayed on-chain by the keyless relayer wallet.");
  } else {
    console.log("\n… consumer not updated (run may still be processing, or no newer round).");
  }
}

// Run the same action via a throwaway webhook trigger fed a synthetic event.
async function simulate(base, token, usage, defaultParams, beforeRound) {
  if (!usage) throw new Error("LIT_USAGE_API_KEY missing — needed to create the temp trigger.");
  const fs = require("fs");
  const path = require("path");
  const actionCode = fs.readFileSync(path.join(__dirname, "..", "action", "feedMirror.js"), "utf8");

  console.log("Creating temporary webhook trigger with the same action...");
  const created = await api(base, token, "POST", "/api/triggers", {
    name: "feed-mirror-simulate",
    kind: "webhook",
    action_code: actionCode,
    default_params: defaultParams,
    usage_api_key: usage,
    config: {},
  });
  const tid = created.id;
  try {
    // Synthetic AnswerUpdated shaped exactly like a real chain_event delivery.
    const synthetic = {
      source: "chain_event",
      chain_key: "base",
      chain_id: 8453,
      decoded: {
        arg0: "200000000000",                 // price (e.g. $2000.00000000)
        arg1: String(beforeRound.add(1)),     // strictly-newer roundId
        arg2: String(Math.floor(Date.now() / 1000) - (Date.now() % 1000 === 0 ? 0 : 1)),
      },
    };
    const queued = await api(base, token, "POST", `/webhook/${tid}`, synthetic, true);
    console.log(`  queued: ${JSON.stringify(queued)}`);
    // We know the exact run id here, so wait for that specific run to finish
    // (don't use the "fresh/seen" baseline — the run already exists by now).
    return await waitForRunById(base, token, tid, queued.run_id);
  } finally {
    await api(base, token, "DELETE", `/api/triggers/${tid}`).catch(() => {});
    console.log("  (temporary trigger deleted)");
  }
}

async function api(base, token, method, path, body, public_ = false) {
  const headers = { "content-type": "application/json" };
  if (!public_) headers.authorization = `Bearer ${token}`;
  const res = await fetch(`${base}${path}`, {
    method, headers, body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (method === "DELETE") return null;
  const out = await res.json().catch(() => null);
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status}: ${JSON.stringify(out)}`);
  return out;
}

async function waitForRunById(base, token, triggerId, runId) {
  for (let i = 0; i < 60; i++) {
    await new Promise((r) => setTimeout(r, 4000));
    const r = (await runsOf(base, token, triggerId)).find((x) => x.id === runId);
    if (r && (r.status === "success" || r.status === "failed")) return r;
  }
  return null;
}

async function waitForFreshRun(base, token, triggerId) {
  const seen = new Set((await runsOf(base, token, triggerId)).map((r) => r.id));
  for (let i = 0; i < 60; i++) {
    await new Promise((r) => setTimeout(r, 4000));
    for (const r of await runsOf(base, token, triggerId)) {
      if (!seen.has(r.id) && (r.status === "success" || r.status === "failed")) return r;
    }
  }
  return null;
}
async function runsOf(base, token, triggerId) {
  const res = await fetch(`${base}/api/triggers/${triggerId}/runs?limit=5`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!res.ok) return [];
  return (await res.json()).runs || [];
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
