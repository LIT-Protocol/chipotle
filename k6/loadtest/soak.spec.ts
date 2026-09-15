/**
 * Soak test (endurance/stability test) for lit-api-server.
 *
 * Runs a sustained, low-intensity workload for an extended period (default 30m)
 * to surface issues that only appear under prolonged load:
 * - Memory leaks
 * - Resource exhaustion
 * - Gradual performance degradation over time
 *
 * Workload: Each virtual user repeatedly alternates between two Lit Action
 * patterns—(1) encrypt/decrypt round-trip with a random challenge, and
 * (2) ECDSA sign ("Chipotle Rocks!") using Lit Action private key—with
 * 2–4 seconds between iterations.
 *
 * Scenarios run in parallel so metrics are separated:
 *   - soak_encrypt_decrypt / ramp_encrypt_decrypt
 *   - soak_ecdsa_sign / ramp_ecdsa_sign
 *
 * Usage:
 *   k6 run k6/loadtest/soak.spec.ts
 *   SOAK_DURATION=30m k6 run k6/loadtest/soak.spec.ts
 *   BASE_URL=http://localhost:8000/core/v1 k6 run k6/loadtest/soak.spec.ts
 *   SCENARIO=ramp k6 run k6/loadtest/soak.spec.ts    # run only ramp scenario
 *
 * Scenarios:
 *   soak_* - Sustained low-intensity workload (default). Ramp up, steady state, ramp down.
 *   ramp_* - Gradual load increase: +1 VU per minute for 8 min, then 1 min ramp-down.
 *
 * Environment:
 *   BASE_URL       - API base URL (default: https://test.chipotle.litprotocol.com/core/v1)
 *   SCENARIO       - Run only this scenario group: "soak" or "ramp" (default: both)
 *   SOAK_DURATION  - Total test duration for soak scenario (default: 30m)
 *   SOAK_VUS       - Virtual users for soak scenario (default: 3)
 */
import { checkAndLog, assertOk } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { PRECREATED_ACCOUNTS, createAccountAndUsageKey } from "../setup.ts";
import { sleep } from "k6";
import {
  ECDSA_SIGN_CODE,
  ENCRYPT_CODE,
  DECRYPT_CODE,
} from "../LitActionCode/index.ts";
import { BASE_URL, COMMON_PARAMS, K6_RUN_ID } from "../defaults.ts";
import { ensureAccountCredits } from "../stripe.ts";
// @ts-ignore – remote JS lib, no type declarations
import { textSummary } from "https://jslib.k6.io/k6-summary/0.1.0/index.js";

// Parse duration: "1h", "30m", "10m" etc.
const SOAK_DURATION = __ENV.SOAK_DURATION || "30m";
const SOAK_VUS = parseInt(__ENV.SOAK_VUS || "3", 10);

// Per-endpoint p95 latency ceilings (ms) for the deploy perf gate.
//
// When a committed baseline is provided (SOAK_BASELINE_FILE — a previously-saved
// soak summary), the ceiling for each endpoint is DERIVED from that baseline's
// p95: fail if this run is more than SOAK_P95_TOLERANCE over it, with a
// SOAK_P95_FLOOR_MS absolute cushion so small-sample jitter doesn't false-fail.
// Without a baseline (local runs, or before one is committed) we fall back to
// these coarse absolute ceilings. All overridable via env.
const SOAK_P95_ENCRYPT_MS = __ENV.SOAK_P95_ENCRYPT_MS || "350";
const SOAK_P95_ECDSA_MS = __ENV.SOAK_P95_ECDSA_MS || "700";

// Regression tolerance vs the baseline: fail if p95 > max(baseline*(1+tol),
// baseline + floor).
const SOAK_P95_TOLERANCE = parseFloat(__ENV.SOAK_P95_TOLERANCE || "0.30");
const SOAK_P95_FLOOR_MS = parseFloat(__ENV.SOAK_P95_FLOOR_MS || "50");

// Committed baseline. When SOAK_BASELINE_FILE is set the gate runs in baseline
// mode and FAILS CLOSED: any problem loading the baseline, an invalid
// tolerance/floor, or a missing/non-positive p95 for a running endpoint throws
// at init so the run errors red. It never silently downgrades to the loose
// absolute ceilings — that would let a real regression pass green. When unset
// (local / measurement runs) we use the absolute SOAK_P95_*_MS ceilings.
// open() resolves relative to THIS spec's dir (k6/loadtest/), so the gate
// passes e.g. "../baselines/soak.next.json".
const SOAK_BASELINE_FILE = __ENV.SOAK_BASELINE_FILE || "";
const baselineMode = SOAK_BASELINE_FILE !== "";
// deno-lint-ignore no-explicit-any
let soakBaseline: any = null;
if (baselineMode) {
  let raw: string;
  try {
    raw = open(SOAK_BASELINE_FILE) as string;
  } catch (e) {
    throw new Error(
      `soak: SOAK_BASELINE_FILE="${SOAK_BASELINE_FILE}" could not be opened (${String(
        (e as Error).message ?? e,
      )}). The gate fails closed — fix the path (relative to k6/loadtest/) or unset it.`,
    );
  }
  try {
    soakBaseline = JSON.parse(raw);
  } catch (e) {
    throw new Error(
      `soak: baseline ${SOAK_BASELINE_FILE} is not valid JSON (${String(
        (e as Error).message ?? e,
      )}).`,
    );
  }
  if (!Number.isFinite(SOAK_P95_TOLERANCE) || SOAK_P95_TOLERANCE < 0) {
    throw new Error(
      `soak: SOAK_P95_TOLERANCE must be a non-negative number — a fraction, e.g. 0.30 = 30% (not 30); got "${__ENV.SOAK_P95_TOLERANCE}".`,
    );
  }
  if (!Number.isFinite(SOAK_P95_FLOOR_MS) || SOAK_P95_FLOOR_MS < 0) {
    throw new Error(
      `soak: SOAK_P95_FLOOR_MS must be a non-negative number of ms; got "${__ENV.SOAK_P95_FLOOR_MS}".`,
    );
  }
}

// p95 ceiling (ms string for the k6 threshold) for an endpoint. In baseline
// mode it is derived from the baseline p95 and throws if that p95 is absent or
// non-positive (fail closed). Without a baseline it returns the absolute
// fallback ceiling.
function p95Ceiling(scenario: string, absoluteMs: string): string {
  if (!baselineMode) return absoluteMs;
  const base = soakBaseline?.scenarios?.[scenario]?.p95;
  if (typeof base !== "number" || !(base > 0)) {
    throw new Error(
      `soak: baseline ${SOAK_BASELINE_FILE} has no valid p95 for "${scenario}" (got ${JSON.stringify(
        base,
      )}). The gate fails closed — regenerate the baseline with k6/update-soak-baseline.sh.`,
    );
  }
  const ceil = Math.max(
    base * (1 + SOAK_P95_TOLERANCE),
    base + SOAK_P95_FLOOR_MS,
  );
  console.log(
    `soak: ${scenario} p95 ceiling ${ceil.toFixed(0)}ms (baseline ${base}ms +${(SOAK_P95_TOLERANCE * 100).toFixed(0)}%/+${SOAK_P95_FLOOR_MS}ms)`,
  );
  return String(ceil);
}

// Gate mode: create ephemeral accounts in setup() instead of reading the
// pre-seeded pool. The deploy gate runs against a freshly-deployed (possibly
// cold) instance that may not have the committed pool's accounts — relying on
// the pool there produces spurious 401s ("key not recognized") and a false
// rollback. Creating accounts against the actual target box is robust. Default
// off so manual/endurance soaks keep reusing the cheap pre-seeded pool.
const SOAK_CREATE_ACCOUNTS =
  (__ENV.SOAK_CREATE_ACCOUNTS || "").toLowerCase() === "true";

// Stages: 2min ramp-up, (duration - 4min) steady, 2min ramp-down
const RAMP_UP = "2m";
const RAMP_DOWN = "2m";

// Ramp scenario: +1 VU per minute for 8 min, then 1 min ramp-down (max 8 VUs)
const RAMP_MAX_VUS = 8;

const allScenarios = {
  soak_encrypt_decrypt: {
    executor: "ramping-vus",
    exec: "encryptDecrypt",
    startVUs: 0,
    stages: [
      { duration: RAMP_UP, target: Math.max(1, Math.ceil(SOAK_VUS / 2)) },
      { duration: SOAK_DURATION, target: Math.max(1, Math.ceil(SOAK_VUS / 2)) },
      { duration: RAMP_DOWN, target: 0 },
    ],
  },
  soak_ecdsa_sign: {
    executor: "ramping-vus",
    exec: "ecdsaSign",
    startVUs: 0,
    stages: [
      { duration: RAMP_UP, target: Math.max(1, Math.floor(SOAK_VUS / 2)) },
      { duration: SOAK_DURATION, target: Math.max(1, Math.floor(SOAK_VUS / 2)) },
      { duration: RAMP_DOWN, target: 0 },
    ],
  },
  ramp_encrypt_decrypt: {
    executor: "ramping-vus",
    exec: "encryptDecrypt",
    startVUs: 0,
    stages: [
      { duration: "1m", target: 1 },
      { duration: "1m", target: 1 },
      { duration: "1m", target: 2 },
      { duration: "1m", target: 2 },
      { duration: "1m", target: 3 },
      { duration: "1m", target: 3 },
      { duration: "1m", target: 4 },
      { duration: "1m", target: 4 },
      { duration: "1m", target: 0 },
    ],
  },
  ramp_ecdsa_sign: {
    executor: "ramping-vus",
    exec: "ecdsaSign",
    startVUs: 0,
    stages: [
      { duration: "1m", target: 0 },
      { duration: "1m", target: 1 },
      { duration: "1m", target: 1 },
      { duration: "1m", target: 2 },
      { duration: "1m", target: 2 },
      { duration: "1m", target: 3 },
      { duration: "1m", target: 3 },
      { duration: "1m", target: 4 },
      { duration: "1m", target: 0 },
    ],
  },
};

// Fail loud on a typo'd SCENARIO rather than silently running every scenario
// (which would re-enable the heavier ramp_* load on a gate that wanted soak).
const SCENARIO = __ENV.SCENARIO || "";
if (SCENARIO !== "" && SCENARIO !== "soak" && SCENARIO !== "ramp") {
  throw new Error(
    `Invalid SCENARIO="${SCENARIO}"; expected "soak", "ramp", or empty (run all).`,
  );
}
const runSoak = SCENARIO === "" || SCENARIO === "soak";
const runRamp = SCENARIO === "" || SCENARIO === "ramp";

const scenarios = {
  ...(runSoak
    ? {
        soak_encrypt_decrypt: allScenarios.soak_encrypt_decrypt,
        soak_ecdsa_sign: allScenarios.soak_ecdsa_sign,
      }
    : {}),
  ...(runRamp
    ? {
        ramp_encrypt_decrypt: allScenarios.ramp_encrypt_decrypt,
        ramp_ecdsa_sign: allScenarios.ramp_ecdsa_sign,
      }
    : {}),
};

// Build thresholds only for the scenarios that actually run. Defining a
// threshold over a submetric that receives zero samples (e.g. ramp_* when
// SCENARIO=soak) is at best misleading and, on some k6 versions, fails the run
// outright — which would break the gate every time. Keep them in lockstep with
// `scenarios` above.
const thresholds: Record<string, string[]> = {
  http_req_duration: ["p(99)<15000"],
  checks: ["rate>=0.95"],
};
if (runSoak) {
  // soak_* carry the interim per-endpoint p95 regression ceilings (the gate
  // runs SCENARIO=soak); keep the loose p99 "didn't fall over" bound too.
  thresholds["http_req_duration{scenario:soak_encrypt_decrypt}"] = [
    `p(95)<${p95Ceiling("soak_encrypt_decrypt", SOAK_P95_ENCRYPT_MS)}`,
    "p(99)<15000",
  ];
  thresholds["http_req_duration{scenario:soak_ecdsa_sign}"] = [
    `p(95)<${p95Ceiling("soak_ecdsa_sign", SOAK_P95_ECDSA_MS)}`,
    "p(99)<15000",
  ];
}
if (runRamp) {
  thresholds["http_req_duration{scenario:ramp_encrypt_decrypt}"] = ["p(99)<15000"];
  thresholds["http_req_duration{scenario:ramp_ecdsa_sign}"] = ["p(99)<15000"];
}

export const options = {
  scenarios,
  setupTimeout: "3m", // accounts × 2 API calls each; ~6s/call; 3m allows for slow responses
  // Include p(99) so the JSON summary / baseline captures it (default stats omit it).
  summaryTrendStats: ["avg", "min", "med", "max", "p(95)", "p(99)"],
  thresholds,
};

export interface SoakAccountData {
  usageApiKey: string;
  pkpId: string;
}

export type SoakSetupData = SoakAccountData[];

export function setup(): SoakSetupData {
  // Ideal account count = the actual peak concurrent VUs across the running
  // scenarios. Each scenario is clamped to a minimum of 1 VU (see makeStages /
  // Math.max(1, ...) in the scenario targets), so soak always runs 2 VUs even at
  // SOAK_VUS=1 — compute the real peak, don't assume SOAK_VUS. VUs index the pool
  // modulo its length (below), so a smaller pool still works by sharing accounts.
  const soakPeak = runSoak
    ? Math.max(1, Math.ceil(SOAK_VUS / 2)) + Math.max(1, Math.floor(SOAK_VUS / 2))
    : 0;
  const idealAccounts = soakPeak + (runRamp ? RAMP_MAX_VUS : 0);

  // Gate path: create fresh accounts against the actual target box so the run
  // never depends on a pre-seeded pool existing on that instance.
  // createAccountAndUsageKey already funds the account via ensureAccountCredits.
  if (SOAK_CREATE_ACCOUNTS) {
    const created: SoakAccountData[] = [];
    for (let i = 0; i < idealAccounts; i++) {
      const acc = createAccountAndUsageKey({
        accountName: `k6-soak-${K6_RUN_ID}-${i}`,
        accountDescription: "ephemeral k6 soak gate account",
        usageKeyName: `k6-soak-usage-${K6_RUN_ID}-${i}`,
        usageKeyDescription: "ephemeral k6 soak gate usage key",
        setupContext: "soak",
      });
      created.push({ usageApiKey: acc.usageApiKey, pkpId: acc.walletAddress });
    }
    logBillingWallets(created);
    return created;
  }

  // Pre-seeded pool. Need at least one account; if the pool is smaller than the
  // VU count, VUs share accounts (fine at low VU — e.g. a 2-VU prod baseline run
  // against a single funded prod account). Warn so it's not silent.
  if (PRECREATED_ACCOUNTS.length === 0) {
    throw new Error(
      `No pre-created accounts found. Run accounts.seed.spec.ts, point K6_ACCOUNTS_FILE at a pool, or set SOAK_CREATE_ACCOUNTS=true to create ephemeral accounts.`,
    );
  }
  if (PRECREATED_ACCOUNTS.length < idealAccounts) {
    console.warn(
      `soak: only ${PRECREATED_ACCOUNTS.length} pre-created account(s) for ${idealAccounts} VU(s); VUs will share accounts. Fine at low VU; add more accounts to avoid per-account contention at higher VU.`,
    );
  }

  const useCount = Math.min(idealAccounts, PRECREATED_ACCOUNTS.length);
  const accounts: SoakAccountData[] = [];
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  for (let i = 0; i < useCount; i++) {
    const account = PRECREATED_ACCOUNTS[i];
    ensureAccountCredits(client, { "X-Api-Key": account.apiKey });
    accounts.push({ usageApiKey: account.usageApiKey, pkpId: account.walletAddress });
  }
  logBillingWallets(accounts);
  return accounts;
}

/**
 * Log the billing wallet address(es) the run will use. These are public 0x
 * addresses (never the API keys) — they match the Stripe customer's
 * `metadata.wallet_address`, so you can find the customer and grant credits
 * when a prod run runs out of money. Printed at setup so it's at the top of
 * the run log.
 */
function logBillingWallets(accounts: SoakAccountData[]): void {
  const wallets = accounts.map((a) => a.pkpId).join(", ");
  console.log(
    `soak: billing wallet address(es) in use (fund the Stripe customer whose metadata.wallet_address matches one of these): ${wallets}`,
  );
}

export function encryptDecrypt(setupData: SoakSetupData) {
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  const account = setupData[(__VU - 1) % setupData.length];
  const usageKeyHeaders = { "X-Api-Key": account.usageApiKey };

  const challenge =
    Math.random().toString(36).slice(2) + Math.random().toString(36).slice(2);

  const encryptRes = client.litAction(
    { code: ENCRYPT_CODE, js_params: { pkpId: account.pkpId, challenge } },
    usageKeyHeaders,
  );
  if (!assertOk("litAction/encrypt", "POST /lit_action", encryptRes)) {
    sleep(2 + Math.random() * 2);
    return;
  }
  const encryptBody = JSON.parse(encryptRes.response.body as unknown as string);
  if (encryptBody.has_error || typeof encryptBody.response !== "string") {
    sleep(2 + Math.random() * 2);
    return;
  }
  const ciphertext: string = encryptBody.response;

  const decryptRes = client.litAction(
    {
      code: DECRYPT_CODE,
      js_params: { pkpId: account.pkpId, ciphertext },
    },
    usageKeyHeaders,
  );
  assertOk("litAction/decrypt", "POST /lit_action", decryptRes);
  checkAndLog(
    decryptRes.response,
    {
      "decrypt has no error": (r) => {
        try {
          return JSON.parse(r.body as unknown as string).has_error === false;
        } catch {
          return false;
        }
      },
      "decrypted matches challenge": (r) => {
        try {
          return JSON.parse(r.body as unknown as string).response === challenge;
        } catch {
          return false;
        }
      },
    },
    "litAction/decrypt",
  );

  // Low intensity: 2–4 seconds between requests per VU
  sleep(2 + Math.random() * 2);
}

export function ecdsaSign(setupData: SoakSetupData) {
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  const account = setupData[(__VU - 1) % setupData.length];
  const usageKeyHeaders = { "X-Api-Key": account.usageApiKey };

  const litActionRes = client.litAction(
    { code: ECDSA_SIGN_CODE, js_params: null },
    usageKeyHeaders,
  );
  assertOk("litAction/ecdsa-sign", "POST /lit_action", litActionRes);
  checkAndLog(
    litActionRes.response,
    {
      "ecdsa-sign has no error": (r) => {
        try {
          return JSON.parse(r.body as unknown as string).has_error === false;
        } catch {
          return false;
        }
      },
      "ecdsa-sign returns wallet_address and signature": (r) => {
        try {
          const body = JSON.parse(r.body as unknown as string);
          const resp = body.response;
          return (
            resp &&
            typeof resp.wallet_address === "string" &&
            typeof resp.signature === "string"
          );
        } catch {
          return false;
        }
      },
    },
    "litAction/ecdsa-sign",
  );

  // Low intensity: 2–4 seconds between requests per VU
  sleep(2 + Math.random() * 2);
}

/** Extract the trend stats for a (sub)metric, null-safe. */
// deno-lint-ignore no-explicit-any
function trend(data: any, sub: string) {
  const v = data?.metrics?.[sub]?.values;
  if (!v) return null;
  return {
    min: v["min"],
    p50: v["med"],
    p95: v["p(95)"],
    p99: v["p(99)"],
    avg: v["avg"],
    max: v["max"],
  };
}

/**
 * Write a machine-readable summary (soak-summary.json) alongside the text
 * summary. This file is BOTH the per-run artifact for the gate's regression
 * comparison and the exact shape the committed baseline takes — so
 * update-soak-baseline.sh just saves this file as the baseline. Keeps the
 * HTTP-failure warning from the old warnOnHttpFailures.
 */
// deno-lint-ignore no-explicit-any
export function handleSummary(data: any): Record<string, string> {
  const summary = {
    env: __ENV.K6_ENV || "",
    base_url: BASE_URL,
    correlation_id: K6_RUN_ID,
    generated_at: new Date().toISOString(),
    scenarios: {
      soak_encrypt_decrypt: trend(
        data,
        "http_req_duration{scenario:soak_encrypt_decrypt}",
      ),
      soak_ecdsa_sign: trend(data, "http_req_duration{scenario:soak_ecdsa_sign}"),
    },
    checks_rate: data?.metrics?.checks?.values?.rate ?? null,
    http_req_failed_rate: data?.metrics?.http_req_failed?.values?.rate ?? null,
  };

  const failed = data?.metrics?.http_req_failed;
  if (failed && failed.values?.rate > 0) {
    console.warn(
      `\n⚠ WARNING: ${(failed.values.rate * 100).toFixed(1)}% of HTTP requests failed\n`,
    );
  }

  return {
    stdout: textSummary(data, { indent: " ", enableColors: true }),
    "soak-summary.json": JSON.stringify(summary, null, 2),
  };
}
