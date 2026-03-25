/**
 * Spike test for lit-api-server.
 *
 * Sudden load increase to verify the system handles traffic spikes:
 * - Recovery behavior under stress
 * - Error rates and degradation under spike
 * - Whether the system recovers after the spike
 *
 * Usage:
 *   k6 run k6/loadtest/spike.spec.ts
 *   SPIK_VUS=50 k6 run k6/loadtest/spike.spec.ts
 *   BASE_URL=http://localhost:8000/core/v1 k6 run k6/loadtest/spike.spec.ts
 *
 * Environment:
 *   BASE_URL        - API base URL (default: api.dev.litprotocol.com/core/v1)
 *   SPIK_VUS        - Peak virtual users (default: 20)
 *   SPIK_DURATION   - Sustain duration at peak (default: 2m)
 */
import { checkAndLog } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { PRECREATED_ACCOUNTS } from "../setup.ts";
import { assertOk } from "../helpers.ts";
import { sleep } from "k6";
import {
  ECDSA_SIGN_CODE,
  ENCRYPT_CODE,
  DECRYPT_CODE,
} from "../LitActionCode/index.ts";
import { BASE_URL, COMMON_PARAMS } from "../defaults.ts";

const SPIK_VUS = parseInt(__ENV.SPIK_VUS || "25", 10);
const SPIK_DURATION = __ENV.SPIK_DURATION || "1m";

// Sudden ramp: 10s to full load, sustain, 10s ramp-down
const RAMP_UP = "15s";
const RAMP_DOWN = "15s";

export const options = {
  scenarios: {
    encrypt_decrypt: {
      executor: "ramping-vus" as const,
      exec: "encryptDecrypt",
      startVUs: 0,
      stages: [
        { duration: RAMP_UP, target: Math.max(1, Math.ceil(SPIK_VUS / 2)) },
        { duration: SPIK_DURATION, target: Math.max(1, Math.ceil(SPIK_VUS / 2)) },
        { duration: RAMP_DOWN, target: 0 },
      ],
    },
    ecdsa_sign: {
      executor: "ramping-vus" as const,
      exec: "ecdsaSign",
      startVUs: 0,
      stages: [
        { duration: RAMP_UP, target: Math.max(1, Math.floor(SPIK_VUS / 2)) },
        { duration: SPIK_DURATION, target: Math.max(1, Math.floor(SPIK_VUS / 2)) },
        { duration: RAMP_DOWN, target: 0 },
      ],
    },
  },
  thresholds: {
    http_req_failed: ["rate<0.2"],
    http_req_duration: ["p(99)<30000"],
    checks: ["rate>=0.8"],
    "http_req_duration{scenario:encrypt_decrypt}": ["p(99)<30000"],
    "http_req_duration{scenario:ecdsa_sign}": ["p(99)<30000"],
    "http_req_failed{scenario:encrypt_decrypt}": ["rate<0.2"],
    "http_req_failed{scenario:ecdsa_sign}": ["rate<0.2"],
  },
};

export interface SpikeSetupData {
  accounts: { usageApiKey: string; pkpId: string }[];
}

export function setup(): SpikeSetupData {
  if (PRECREATED_ACCOUNTS.length < SPIK_VUS) {
    throw new Error(
      `Not enough pre-created accounts for spike test: need ${SPIK_VUS}, found ${PRECREATED_ACCOUNTS.length}. Run accounts.seed.spec.ts with a higher ACCOUNTS_COUNT.`,
    );
  }

  // Use a distinct account per VU so each VU exercises a different PKP/usage key pair.
  const accounts: { usageApiKey: string; pkpId: string }[] = [];
  for (let i = 0; i < SPIK_VUS; i++) {
    const acc = PRECREATED_ACCOUNTS[i];
    accounts.push({ usageApiKey: acc.usageApiKey, pkpId: acc.walletAddress });
  }

  return { accounts };
}

export function encryptDecrypt(setupData: SpikeSetupData) {
  const client = new LitApiServerClient({
    baseUrl: BASE_URL,
    commonRequestParameters: COMMON_PARAMS,
  });
  const account = setupData.accounts[__VU - 1];
  const usageKeyHeaders = { "X-Api-Key": account.usageApiKey };

  const challenge =
    Math.random().toString(36).slice(2) + Math.random().toString(36).slice(2);

  const encryptRes = client.litAction(
    { code: ENCRYPT_CODE, js_params: { pkpId: account.pkpId, challenge } },
    usageKeyHeaders,
  );
  if (!assertOk("litAction/encrypt", "POST /lit_action", encryptRes)) {
    sleep(0.2 + Math.random() * 0.2);
    return;
  }
  const encryptBody = JSON.parse(encryptRes.response.body as unknown as string);
  if (encryptBody.has_error || typeof encryptBody.response !== "string") {
    sleep(0.2 + Math.random() * 0.2);
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

  // High intensity: short pause between spike requests.
  sleep(0.2 + Math.random() * 0.2);
}

export function ecdsaSign(setupData: SpikeSetupData) {
  const client = new LitApiServerClient({
    baseUrl: BASE_URL,
    commonRequestParameters: COMMON_PARAMS,
  });
  const account = setupData.accounts[__VU - 1];
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

  // High intensity: short pause between spike requests.
  sleep(0.2 + Math.random() * 0.2);
}
