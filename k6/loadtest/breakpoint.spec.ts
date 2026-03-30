/**
 * Breakpoint test for lit-api-server.
 *
 * Gradually increases load in steps up to a maximum number of VUs (default 50)
 * so you can observe where latency, error rates, or resource usage starts to
 * become unacceptable (the "breakpoint").
 *
 * Two scenarios run in parallel, each ramping through ~half the VU targets:
 *   - encrypt_decrypt: encrypt/decrypt round-trip with a random challenge
 *   - ecdsa_sign: ECDSA signature via Lit Action
 *
 * Default stages (if BPT_STEPS is not provided) are:
 *   1 VU → 5 VUs → 10 VUs → 15 VUs → 20 VUs → 25 VUs (per scenario)
 *
 * Usage (from repo root):
 *   k6 run k6/loadtest/breakpoint.spec.ts
 *   BPT_MAX_VUS=40 k6 run k6/loadtest/breakpoint.spec.ts
 *   BPT_STEP_DURATION=3m k6 run k6/loadtest/breakpoint.spec.ts
 *   BPT_STEPS=1,3,6,12,24 k6 run k6/loadtest/breakpoint.spec.ts
 *   BASE_URL=http://localhost:8000/core/v1 k6 run k6/loadtest/breakpoint.spec.ts
 *
 * Environment:
 *   BASE_URL          - API base URL (default: api.dev.litprotocol.com/core/v1)
 *   BPT_MAX_VUS       - Max/peak virtual users across both scenarios (default: 50)
 *   BPT_STEP_DURATION - Duration for each load step (default: 2m)
 *   BPT_STEPS         - Comma-separated list of VU levels; last one is treated
 *                       as the breakpoint/max. If omitted, a sensible default
 *                       sequence up to BPT_MAX_VUS is used.
 */
import { checkAndLog, assertOk, warnOnHttpFailures } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { PRECREATED_ACCOUNTS } from "../setup.ts";
import { sleep } from "k6";
import {
  ECDSA_SIGN_CODE,
  ENCRYPT_CODE,
  DECRYPT_CODE,
} from "../LitActionCode/index.ts";
import { BASE_URL, COMMON_PARAMS } from "../defaults.ts";
import { ensureAccountCredits } from "../stripe.ts";

const BPT_MAX_VUS = parseInt(__ENV.BPT_MAX_VUS || "50", 10);
const BPT_STEP_DURATION = __ENV.BPT_STEP_DURATION || "2m";

// Optional explicit steps, e.g. "1,3,6,12,24".
// We sanitize to positive integers, sorted ascending, and clamp to BPT_MAX_VUS.
function parseSteps(): number[] {
  const raw = __ENV.BPT_STEPS;
  if (!raw) {
    const defaults = [1, 10, 20, 30, 40, BPT_MAX_VUS].filter(
      (v, idx, arr) => v > 0 && v <= BPT_MAX_VUS && arr.indexOf(v) === idx,
    );
    return defaults.length > 0 ? defaults : [BPT_MAX_VUS];
  }

  const parsed = raw
    .split(",")
    .map((s) => parseInt(s.trim(), 10))
    .filter((n) => Number.isFinite(n) && n > 0)
    .map((n) => Math.min(n, BPT_MAX_VUS));

  // Deduplicate and sort ascending
  const uniqueSorted: number[] = Array.from(new Set<number>(parsed)).sort(
    (a, b) => a - b,
  );
  return uniqueSorted.length > 0 ? uniqueSorted : [BPT_MAX_VUS];
}

const STEPS = parseSteps();

// Each scenario gets approximately half the VUs at each step so total load ≈ BPT_MAX_VUS.
const ENCRYPT_DECRYPT_STEPS = STEPS.map((v) => Math.max(1, Math.ceil(v / 2)));
const ECDSA_SIGN_STEPS = STEPS.map((v) => Math.max(1, Math.floor(v / 2)));

function makeStages(steps: number[]) {
  return [
    ...steps.map((target) => ({
      duration: BPT_STEP_DURATION,
      target,
    })),
    // Final cool-down period back to zero load so we can observe recovery.
    { duration: BPT_STEP_DURATION, target: 0 },
  ];
}

export const options = {
  scenarios: {
    encrypt_decrypt: {
      executor: "ramping-vus" as const,
      exec: "encryptDecrypt",
      startVUs: 0,
      stages: makeStages(ENCRYPT_DECRYPT_STEPS),
    },
    ecdsa_sign: {
      executor: "ramping-vus" as const,
      exec: "ecdsaSign",
      startVUs: 0,
      stages: makeStages(ECDSA_SIGN_STEPS),
    },
  },
  thresholds: {
    http_req_duration: [{ threshold: "p(95)<2000", abortOnFail: true }],
    checks: [{ threshold: "rate>=0.9", abortOnFail: true }],
    "http_req_duration{scenario:encrypt_decrypt}": ["p(95)<2000"],
    "http_req_duration{scenario:ecdsa_sign}": ["p(95)<2000"],
  },
};

export interface BreakpointSetupData {
  accounts: { usageApiKey: string; pkpId: string }[];
}

export function setup(): BreakpointSetupData {
  if (PRECREATED_ACCOUNTS.length < BPT_MAX_VUS) {
    throw new Error(
      `Not enough pre-created accounts for breakpoint test: need ${BPT_MAX_VUS}, found ${PRECREATED_ACCOUNTS.length}. Run accounts.seed.spec.ts with a higher ACCOUNTS_COUNT.`,
    );
  }

  const accounts: { usageApiKey: string; pkpId: string }[] = [];
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  for (let i = 0; i < BPT_MAX_VUS; i++) {
    const acc = PRECREATED_ACCOUNTS[i];
    ensureAccountCredits(client, { "X-Api-Key": acc.apiKey });
    accounts.push({ usageApiKey: acc.usageApiKey, pkpId: acc.walletAddress });
  }

  return { accounts };
}

export function encryptDecrypt(setupData: BreakpointSetupData) {
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
    sleep(0.5 + Math.random() * 0.5);
    return;
  }
    const encryptBody = JSON.parse(encryptRes.response.body as unknown as string);
  if (encryptBody.has_error || typeof encryptBody.response !== "string") {
    sleep(0.5 + Math.random() * 0.5);
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

  sleep(0.5 + Math.random() * 0.5);
}

export function ecdsaSign(setupData: BreakpointSetupData) {
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

  sleep(0.5 + Math.random() * 0.5);
}

export const handleSummary = warnOnHttpFailures;
