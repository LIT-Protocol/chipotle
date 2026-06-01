// End-to-end client: watch the schedule trigger pay out.
//
// Records the policyholder's balance, waits for the next scheduled run, shows
// the payout result + balance delta, then DISABLES the trigger so the demo pool
// doesn't keep draining every minute. Re-enable from the dashboard or with a
// PATCH if you want it to keep running.

const { ethers } = require("ethers");
const env = require("./_env");

async function main() {
  env.load();
  const {
    TRIGGERS_BASE = "https://triggers.litprotocol.com",
    LIT_TRIGGERS_AGENT_TOKEN: TOKEN,
    TRIGGER_ID,
    POOL_WALLET_ADDRESS,
    BASE_SEPOLIA_RPC_URL = "https://sepolia.base.org",
    DEPLOYER_PRIVATE_KEY,
    POLICYHOLDER,
  } = process.env;

  for (const k of ["LIT_TRIGGERS_AGENT_TOKEN", "TRIGGER_ID", "POOL_WALLET_ADDRESS"]) {
    if (!process.env[k]) throw new Error(`${k} missing from .env — run \`npm run setup\` first.`);
  }

  const provider = new ethers.providers.JsonRpcProvider(BASE_SEPOLIA_RPC_URL);
  const policyholder = POLICYHOLDER || new ethers.Wallet(DEPLOYER_PRIVATE_KEY).address;

  const before = await provider.getBalance(policyholder);
  console.log(`Policyholder ${policyholder}`);
  console.log(`  balance before: ${ethers.utils.formatEther(before)} ETH`);
  console.log(`Pool ${POOL_WALLET_ADDRESS}: ${ethers.utils.formatEther(await provider.getBalance(POOL_WALLET_ADDRESS))} ETH`);

  // setup leaves the trigger disabled so it can't drain the pool. Enable it,
  // catch one scheduled payout, then disable again (in the finally below).
  console.log("Enabling the trigger and waiting for the next scheduled run (cron tick)...");
  await patch(TRIGGERS_BASE, TOKEN, TRIGGER_ID, { enabled: true });

  let run;
  try {
    run = await waitForFreshRun(TRIGGERS_BASE, TOKEN, TRIGGER_ID);
  } finally {
    await patch(TRIGGERS_BASE, TOKEN, TRIGGER_ID, { enabled: false });
  }
  const inner = run && run.response && run.response.response;
  console.log(`  run status: ${run && run.status}`);
  console.log(`  action result: ${JSON.stringify(inner)}`);

  if (inner && inner.paid && inner.txHash) {
    await provider.waitForTransaction(inner.txHash, 1, 60000).catch(() => {});
    const after = await provider.getBalance(policyholder);
    console.log(`  payout tx: ${inner.txHash}`);
    console.log(`  balance after:  ${ethers.utils.formatEther(after)} ETH`);
    console.log(`  delta:          +${ethers.utils.formatEther(after.sub(before))} ETH`);
    console.log("\n✓ Parametric payout executed autonomously by the keyless pool wallet.");
  } else {
    console.log("\n… no payout this run (service reported healthy, or dryRun).");
  }

  // Stop the demo pool from draining every minute.
  await patch(TRIGGERS_BASE, TOKEN, TRIGGER_ID, { enabled: false });
  console.log("Disabled the trigger (demo cleanup). Re-enable to keep it running.");
}

async function waitForFreshRun(base, token, triggerId) {
  // baseline: ignore runs that already exist
  const seen = new Set((await runs(base, token, triggerId)).map((r) => r.id));
  for (let i = 0; i < 50; i++) {
    await new Promise((r) => setTimeout(r, 4000));
    for (const r of await runs(base, token, triggerId)) {
      if (!seen.has(r.id) && (r.status === "success" || r.status === "failed")) return r;
    }
  }
  return null;
}
async function runs(base, token, triggerId) {
  const res = await fetch(`${base}/api/triggers/${triggerId}/runs?limit=5`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!res.ok) return [];
  return (await res.json()).runs || [];
}
async function patch(base, token, triggerId, body) {
  await fetch(`${base}/api/triggers/${triggerId}`, {
    method: "PATCH",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
