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
import { checkAndLog, assertOk, warnOnHttpFailures } from "../helpers.ts";
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

// Parse duration: "1h", "30m", "10m" etc.
const SOAK_DURATION = __ENV.SOAK_DURATION || "30m";
const SOAK_VUS = parseInt(__ENV.SOAK_VUS || "3", 10);

// Interim per-endpoint p95 latency ceilings (ms) for the deploy gate.
// Coarse absolute ceilings at ~3x the observed low-load baseline on staging
// (encrypt/decrypt ~106ms p95, ecdsa-sign ~274ms p95) — enough to catch gross
// "we made this endpoint a lot slower" regressions without false-failing on
// normal jitter. Phase 2 replaces these with run-over-run deltas vs a stored
// baseline. Overridable via env so manual/endurance soaks at higher VU (where
// latency legitimately rises) don't trip them.
const SOAK_P95_ENCRYPT_MS = __ENV.SOAK_P95_ENCRYPT_MS || "350";
const SOAK_P95_ECDSA_MS = __ENV.SOAK_P95_ECDSA_MS || "700";

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
    `p(95)<${SOAK_P95_ENCRYPT_MS}`,
    "p(99)<15000",
  ];
  thresholds["http_req_duration{scenario:soak_ecdsa_sign}"] = [
    `p(95)<${SOAK_P95_ECDSA_MS}`,
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
  thresholds,
};

export interface SoakAccountData {
  usageApiKey: string;
  pkpId: string;
}

export type SoakSetupData = SoakAccountData[];

export function setup(): SoakSetupData {
  // Only provision/fund accounts for the scenarios that actually run. k6 gives
  // each concurrent VU a globally-unique __VU, so the pool must cover the SUM of
  // the running scenarios' peak VUs (soak peaks at SOAK_VUS total across its two
  // scenarios; ramp peaks at RAMP_MAX_VUS). With SCENARIO=soak,SOAK_VUS=2 this is
  // 2 — not 8 — so a soak-only gate doesn't demand or fund ramp accounts it never
  // uses (which would waste Stripe top-ups and fail if the pool is < 8).
  const requiredAccounts =
    (runSoak ? SOAK_VUS : 0) + (runRamp ? RAMP_MAX_VUS : 0);

  // Gate path: create fresh accounts against the actual target box so the run
  // never depends on a pre-seeded pool existing on that instance.
  // createAccountAndUsageKey already funds the account via ensureAccountCredits.
  if (SOAK_CREATE_ACCOUNTS) {
    const created: SoakAccountData[] = [];
    for (let i = 0; i < requiredAccounts; i++) {
      const acc = createAccountAndUsageKey({
        accountName: `k6-soak-${K6_RUN_ID}-${i}`,
        accountDescription: "ephemeral k6 soak gate account",
        usageKeyName: `k6-soak-usage-${K6_RUN_ID}-${i}`,
        usageKeyDescription: "ephemeral k6 soak gate usage key",
        setupContext: "soak",
      });
      created.push({ usageApiKey: acc.usageApiKey, pkpId: acc.walletAddress });
    }
    return created;
  }

  if (PRECREATED_ACCOUNTS.length < requiredAccounts) {
    throw new Error(
      `Not enough pre-created accounts for soak test: need ${requiredAccounts}, found ${PRECREATED_ACCOUNTS.length}. Run accounts.seed.spec.ts with a higher ACCOUNTS_COUNT, or set SOAK_CREATE_ACCOUNTS=true to create ephemeral accounts.`,
    );
  }

  const accounts: SoakAccountData[] = [];
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  for (let i = 0; i < requiredAccounts; i++) {
    const account = PRECREATED_ACCOUNTS[i];
    ensureAccountCredits(client, { "X-Api-Key": account.apiKey });
    accounts.push({ usageApiKey: account.usageApiKey, pkpId: account.walletAddress });
  }
  return accounts;
}

export function encryptDecrypt(setupData: SoakSetupData) {
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  const account = setupData[__VU - 1];
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
  const account = setupData[__VU - 1];
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

export const handleSummary = warnOnHttpFailures;
