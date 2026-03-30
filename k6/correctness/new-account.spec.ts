/**
 * Correctness test: creates new accounts concurrently.
 * Verifies account creation succeeds under light concurrency (2 VUs).
 *
 * Use: k6 run k6/correctness/new-account.spec.ts
 *      BASE_URL=https://your-instance.phala.network/core/v1 k6 run k6/correctness/new-account.spec.ts
 */
import { sleep } from "k6";
import { checkAndLog, warnOnHttpFailures } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { assertOk } from "../helpers.ts";
import { BASE_URL, COMMON_PARAMS, K6_RUN_ID } from "../defaults.ts";

export const options = {
  vus: 2,
  iterations: 4,
  thresholds: {
    http_reqs: ["count>=1"],
    checks: ["rate==1"],
  },
};

export function setup() {
  console.log(`k6 run correlation id: ${K6_RUN_ID}`);
}

export default function () {
  // Stagger start to reduce simultaneous blockchain tx submissions (avoids nonce conflicts)
  sleep(Math.random() * 2);

  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });

  // Unique account name per VU/iteration to avoid conflicts under concurrency
  const accountName = `k6-new-account-vu${__VU}-i${__ITER}-${Date.now()}`;

  const newAccountRes = client.newAccount({
    account_name: accountName,
    account_description: "k6 new-account correctness test",
  });

  if (!assertOk("newAccount", "POST /new_account", newAccountRes)) return;

  const data = newAccountRes.data as { api_key: string; wallet_address: string };
  checkAndLog(newAccountRes.response, {
    "newAccount returns api_key": () =>
      typeof data.api_key === "string" && data.api_key.length > 0,
    "newAccount returns wallet_address": () =>
      typeof data.wallet_address === "string" && data.wallet_address.length > 0,
  }, "newAccount");
}

export const handleSummary = warnOnHttpFailures;
