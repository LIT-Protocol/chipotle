/** Sequential diagnostics alongside the soak gate; no measured retries. */
import { LitApiServerClient } from "../litApiServer.ts";
import { createAccountAndUsageKey } from "../setup.ts";
import { BASE_URL, COMMON_PARAMS, K6_RUN_ID } from "../defaults.ts";
import { sleep } from "k6";
import { Trend } from "k6/metrics";
import { sha256 } from "k6/crypto";
import { publicResult } from "../../performance/relay/samples.mjs";

const relay = open("../../performance/relay/fixtures/relay.js");
const paramsText = open("../../performance/relay/fixtures/sign-params.json");
const signParams = JSON.parse(paramsText);
const benchmarkHash = sha256(open("./relay.spec.ts"), "hex");
const clientHash = sha256(open("../litApiServer.ts"), "hex");
const message = "Chipotle benchmark: signature only, not a transaction";
const sources = {
  hello: 'async function main() { return "benchmark-ok"; }',
  action_sign: `async function main() { const w = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey()); return {address:w.address, signature:await w.signMessage(${JSON.stringify(message)})}; }`,
  pkp_sign: `async function main({pkpId}) { const w = new ethers.Wallet(await Lit.Actions.getPrivateKey({pkpId})); return {address:w.address, signature:await w.signMessage(${JSON.stringify(message)})}; }`,
  relay_wallet: relay.trim(),
  relay_sign: relay.trim(),
};
const timings = Object.fromEntries(
  Object.keys(sources).map((op) => [op, new Trend(`benchmark_${op}`, true)]),
);
const client = () =>
  new LitApiServerClient({
    baseUrl: BASE_URL,
    commonRequestParameters: COMMON_PARAMS,
  });
export const options = {
  vus: 1,
  iterations: 1,
  setupTimeout: "8m",
  teardownTimeout: "2m",
  summaryTrendStats: ["med", "p(95)", "min", "max"],
  thresholds: { iterations: ["count==1"] },
};
function data(r) {
  if (r.response.status < 200 || r.response.status >= 300)
    throw new Error(`Benchmark HTTP ${r.response.status}`);
  return r.data;
}
export function setup() {
  if (__ENV.K6_ENV !== "staging")
    throw new Error("Benchmark provisioning requires staging");
  const c = client();
  if (!data(c.billingStripeConfig()).publishable_key?.startsWith("pk_test_"))
    throw new Error("Stripe sandbox required");
  const a = createAccountAndUsageKey({
    accountName: K6_RUN_ID,
    accountDescription: "Sequential benchmark",
    usageKeyName: K6_RUN_ID,
    usageKeyDescription: "Sequential benchmark",
  });
  const headers = { "X-Api-Key": a.apiKey };
  const pkp = data(c.createWalletPost(headers)).wallet_address;
  if (!/^0x[0-9a-f]{40}$/i.test(pkp))
    throw new Error("Invalid provisioned wallet");
  let group;
  try {
    group = data(
      c.addGroup(
        {
          group_name: K6_RUN_ID,
          group_description: "Benchmark permissions",
          pkp_ids_permitted: [pkp],
          cid_hashes_permitted: [],
        },
        headers,
      ),
    ).group_id;
    if (group === undefined) throw new Error("Missing permission group");
    for (const source of new Set(Object.values(sources))) {
      const cid = data(c.getLitActionIpfsId(source));
      data(
        c.addActionToGroup(
          { group_id: Number(group), action_ipfs_cid: cid },
          headers,
        ),
      );
    }
    const version = data(c.getVersion());
    const account = { ...a, pkp, group, nodeVersion: version.commit_version };
    // Permission propagation is readiness work, excluded from benchmark timings.
    for (const op of Object.keys(sources)) {
      const end = Date.now() + 60_000;
      for (;;) {
        try {
          publicResult(op, data(execute(c, account, op)));
          break;
        } catch {}
        if (Date.now() > end) throw new Error(`${op}: readiness timed out`);
        sleep(1);
      }
    }
    return account;
  } catch (e) {
    if (group !== undefined)
      data(c.removeGroup({ group_id: String(group) }, headers));
    throw e;
  }
}
function execute(c, a, op) {
  return c.litAction(
    {
      code: sources[op],
      js_params:
        op === "relay_sign"
          ? { ...signParams, pkpId: a.pkp }
          : { pkpId: a.pkp, action: "wallet" },
    },
    { "X-Api-Key": a.usageApiKey },
  );
}
export default function (a) {
  const c = client();
  console.log(
    `RELAY_META ${JSON.stringify({ baseUrl: BASE_URL, correlationId: K6_RUN_ID, pkpId: a.pkp, nodeVersion: a.nodeVersion, relayFixtureSha256: sha256(relay, "hex"), paramsSha256: sha256(paramsText, "hex"), benchmarkSha256: benchmarkHash, clientSha256: clientHash, actionHashes: Object.fromEntries(Object.entries(sources).map(([name, source]) => [name, sha256(source, "hex")])), startedAt: new Date().toISOString() })}`,
  );
  for (const op of Object.keys(sources)) {
    for (let i = 0; i < 12; i++) {
      const startedAt = new Date().toISOString(),
        start = Date.now();
      const r = execute(c, a, op),
        clientMs = Date.now() - start;
      const sample = {
        operation: op,
        index: i,
        warmup: i < 2,
        startedAt,
        clientMs,
        requestId: r.response.headers["X-Request-Id"],
        body: publicResult(op, data(r)),
      };
      // publicResult strips logs and rejects unknown/malformed output before logging.
      // The offline verifier checks signatures and completeness, outside the timer.
      console.log(`RELAY_SAMPLE ${JSON.stringify(sample)}`);
      if (!sample.requestId) throw new Error(`${op}: missing request ID`);
      if (i >= 2) timings[op].add(clientMs);
    }
  }
  console.log(
    `RELAY_DONE ${JSON.stringify({ completedAt: new Date().toISOString() })}`,
  );
}
export function teardown(a) {
  if (a?.group !== undefined) {
    data(
      client().removeGroup(
        { group_id: String(a.group) },
        { "X-Api-Key": a.apiKey },
      ),
    );
    console.log("RELAY_CLEANUP {}");
  }
}
