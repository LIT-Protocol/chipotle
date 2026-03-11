/**
 * High-concurrency test: creates new accounts only.
 * Intended for load testing account creation (e.g. 10 VUs) without failure.
 *
 * Use: k6 run k6/new-account.spec.ts
 *      BASE_URL=https://your-instance.phala.network/core/v1 k6 run k6/new-account.spec.ts
 */
import type { Response } from "k6/http";
import { sleep } from "k6";
import { checkAndLog } from "./check.ts";
import { LitApiServerClient } from "./litApiServer.ts";

const baseUrl =
  __ENV.BASE_URL ||
  "https://e364da71b0c9af3b9068daa6321edd6ee932aa89-8000.dstack-pha-prod5.phala.network/core/v1";

export const options = {
  vus: 2,
  iterations: 10,
  thresholds: {
    http_req_failed: ["rate==0"],
    http_reqs: ["count>=10"],
    checks: ["rate==1"],
  },
};

export default function () {
  // Stagger start to reduce simultaneous blockchain tx submissions (avoids nonce conflicts)
  sleep(Math.random() * 2);

  const client = new LitApiServerClient({ baseUrl });

  // Unique account name per VU/iteration to avoid conflicts under concurrency
  const accountName = `k6-new-account-vu${__VU}-i${__ITER}-${Date.now()}`;

  const newAccountRes = client.newAccount({
    account_name: accountName,
    account_description: "k6 new-account load test",
    initial_balance: "10000",
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

function assertOk(
  name: string,
  endpoint: string,
  res: { response: Response },
): boolean {
  const { response } = res;
  const status = response?.status ?? 0;
  const ok = status >= 200 && status < 300;
  if (!ok) {
    let msg = "";
    if (status === 0) {
      msg = "(no response / connection failed)";
    } else {
      try {
        const body = JSON.parse(response.body as string);
        msg =
          body.message ??
          body.error ??
          body.detail ??
          (typeof body === "string" ? body : JSON.stringify(body));
      } catch {
        msg = (response.body as string) || "(no body)";
      }
    }
    console.error(`FAIL ${name} | ${endpoint} | ${status} | ${msg}`);
  }
  checkAndLog(response, {
    [`${name} 2xx`]: (r) =>
      (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, name);
  return ok;
}
