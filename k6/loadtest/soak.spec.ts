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
 *   BASE_URL       - API base URL (default: api.dev.litprotocol.com/core/v1)
 *   SCENARIO       - Run only this scenario group: "soak" or "ramp" (default: both)
 *   SOAK_DURATION  - Total test duration for soak scenario (default: 30m)
 *   SOAK_VUS       - Virtual users for soak scenario (default: 3)
 */
import { checkAndLog, assertOk } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { PRECREATED_ACCOUNTS } from "../setup.ts";
import { sleep } from "k6";
import {
  ECDSA_SIGN_CODE,
  ENCRYPT_CODE,
  DECRYPT_CODE,
} from "../LitActionCode/index.ts";
import { BASE_URL, COMMON_PARAMS } from "../defaults.ts";
import { topUpAccount, isBillingEnabled } from "../stripe.ts";

// Parse duration: "1h", "30m", "10m" etc.
const SOAK_DURATION = __ENV.SOAK_DURATION || "30m";
const SOAK_VUS = parseInt(__ENV.SOAK_VUS || "3", 10);

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

const SCENARIO = __ENV.SCENARIO;
const scenarios =
  SCENARIO === "soak"
    ? {
        soak_encrypt_decrypt: allScenarios.soak_encrypt_decrypt,
        soak_ecdsa_sign: allScenarios.soak_ecdsa_sign,
      }
    : SCENARIO === "ramp"
      ? {
          ramp_encrypt_decrypt: allScenarios.ramp_encrypt_decrypt,
          ramp_ecdsa_sign: allScenarios.ramp_ecdsa_sign,
        }
      : allScenarios;

export const options = {
  scenarios,
  setupTimeout: "3m", // 8 accounts × 2 API calls each; ~6s/call → ~96s min; 3m allows for slow responses
  thresholds: {
    http_req_failed: ["rate<0.05"],
    http_req_duration: ["p(99)<15000"],
    checks: ["rate>=0.95"],
    "http_req_duration{scenario:soak_encrypt_decrypt}": ["p(99)<15000"],
    "http_req_duration{scenario:soak_ecdsa_sign}": ["p(99)<15000"],
    "http_req_duration{scenario:ramp_encrypt_decrypt}": ["p(99)<15000"],
    "http_req_duration{scenario:ramp_ecdsa_sign}": ["p(99)<15000"],
    "http_req_failed{scenario:soak_encrypt_decrypt}": ["rate<0.05"],
    "http_req_failed{scenario:soak_ecdsa_sign}": ["rate<0.05"],
    "http_req_failed{scenario:ramp_encrypt_decrypt}": ["rate<0.05"],
    "http_req_failed{scenario:ramp_ecdsa_sign}": ["rate<0.05"],
  },
};

export interface SoakAccountData {
  usageApiKey: string;
  pkpId: string;
}

export type SoakSetupData = SoakAccountData[];

export function setup(): SoakSetupData {
  const maxVus = Math.max(SOAK_VUS, RAMP_MAX_VUS);
  if (PRECREATED_ACCOUNTS.length < maxVus) {
    throw new Error(
      `Not enough pre-created accounts for soak test: need ${maxVus}, found ${PRECREATED_ACCOUNTS.length}. Run accounts.seed.spec.ts with a higher ACCOUNTS_COUNT.`,
    );
  }

  const accounts: SoakAccountData[] = [];
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  const billing = isBillingEnabled(client);
  for (let i = 0; i < maxVus; i++) {
    const account = PRECREATED_ACCOUNTS[i];
    if (billing) {
      topUpAccount(client, { "X-Api-Key": account.apiKey });
    }
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
