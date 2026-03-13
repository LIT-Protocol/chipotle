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
 * 2–4 seconds between iterations. Odd VUs run encrypt/decrypt; even VUs
 * run ECDSA sign.
 *
 * Usage:
 *   k6 run k6/loadtest/soak.spec.ts
 *   SOAK_DURATION=30m k6 run k6/loadtest/soak.spec.ts
 *   BASE_URL=http://localhost:8000/core/v1 k6 run k6/loadtest/soak.spec.ts
 *
 * Environment:
 *   BASE_URL       - API base URL (default: api.dev.litprotocol.com/core/v1)
 *   SOAK_DURATION  - Total test duration (default: 1h)
 *   SOAK_VUS       - Virtual users (default: 3)
 */
import { checkAndLog } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { createAccountAndUsageKey } from "../setup.ts";
import { assertOk } from "../helpers.ts";
import { sleep } from "k6";
import {
  ECDSA_SIGN_CODE,
  ENCRYPT_CODE,
  DECRYPT_CODE,
} from "../LitActionCode/index.ts";
import { BASE_URL } from "../defaults.ts";

// Parse duration: "1h", "30m", "10m" etc.
const SOAK_DURATION = __ENV.SOAK_DURATION || "30m";
const SOAK_VUS = parseInt(__ENV.SOAK_VUS || "3", 10);

// Stages: 2min ramp-up, (duration - 4min) steady, 2min ramp-down
const RAMP_UP = "2m";
const RAMP_DOWN = "2m";

export const options = {
  vus: SOAK_VUS,
  stages: [
    { duration: RAMP_UP, target: SOAK_VUS },
    { duration: SOAK_DURATION, target: SOAK_VUS },
    { duration: RAMP_DOWN, target: 0 },
  ],
  thresholds: {
    http_req_failed: ["rate<0.05"],
    http_req_duration: ["p(99)<15000"],
    checks: ["rate>=0.95"],
  },
};

export interface SoakAccountData {
  usageApiKey: string;
  pkpId: string;
}

export type SoakSetupData = SoakAccountData[];

export function setup(): SoakSetupData {
  const accounts: SoakAccountData[] = [];

  for (let i = 0; i < SOAK_VUS; i++) {
    const { usageApiKey, walletAddress } = createAccountAndUsageKey({
      accountName: `k6-soak-test-vu-${i + 1}`,
      accountDescription: `k6 soak test account VU ${i + 1}`,
      usageKeyName: `k6-soak-usage-key-vu-${i + 1}`,
      usageKeyDescription: `k6 soak test usage key VU ${i + 1}`,
      setupContext: `soak VU ${i + 1}`,
    });
    accounts.push({ usageApiKey, pkpId: walletAddress });
  }

  return accounts;
}

export default function (setupData: SoakSetupData) {
  const client = new LitApiServerClient({ baseUrl: BASE_URL });
  const account = setupData[__VU - 1];
  const usageKeyHeaders = { "X-Api-Key": account.usageApiKey };

  // Alternate between encrypt/decrypt lit action and ECDSA sign lit action
  const useEncryptDecrypt = __VU % 2 === 1; // VU 1,3,... use encrypt/decrypt; VU 2,4,... use ECDSA sign

  if (useEncryptDecrypt) {
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
    const encryptBody = JSON.parse(encryptRes.response.body as string);
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
    checkAndLog(decryptRes.response, {
      "decrypt has no error": (r) => {
        try {
          return JSON.parse(r.body as string).has_error === false;
        } catch {
          return false;
        }
      },
      "decrypted matches challenge": (r) => {
        try {
          return JSON.parse(r.body as string).response === challenge;
        } catch {
          return false;
        }
      },
    }, "litAction/decrypt");
  } else {
    const litActionRes = client.litAction(
      { code: ECDSA_SIGN_CODE, js_params: null },
      usageKeyHeaders,
    );
    assertOk("litAction/ecdsa-sign", "POST /lit_action", litActionRes);
    checkAndLog(litActionRes.response, {
      "ecdsa-sign has no error": (r) => {
        try {
          return JSON.parse(r.body as string).has_error === false;
        } catch {
          return false;
        }
      },
      "ecdsa-sign returns wallet_address and signature": (r) => {
        try {
          const body = JSON.parse(r.body as string);
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
    }, "litAction/ecdsa-sign");
  }

  // Low intensity: 2–4 seconds between requests per VU
  sleep(2 + Math.random() * 2);
}
