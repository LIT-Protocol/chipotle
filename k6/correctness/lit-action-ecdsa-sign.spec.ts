/**
 * Tests Lit.Actions.getLitActionPrivateKey() and ethers signMessage — runs the
 * ecdsa-sign Lit Action and asserts successful execution.
 *
 * Setup: create account and usage API key.
 * Default: run ecdsa-sign Lit Action, check for success.
 *
 * Usage:
 *   k6 run k6/correctness/lit-action-ecdsa-sign.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/correctness/lit-action-ecdsa-sign.spec.ts
 */
import type { Response } from "k6/http";
import { checkAndLog } from "../check.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { ECDSA_SIGN_CODE } from "../LitActionCode/index.ts";
import { BASE_URL } from "../defaults.ts";

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_req_failed: ["rate<0.1"],
    http_req_duration: ["p(99)<30000"],
    http_reqs: ["count>=1"],
    checks: ["rate==1"],
  },
};

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

export function setup() {
  const client = new LitApiServerClient({ baseUrl: BASE_URL });

  const newAccountRes = client.newAccount({
    account_name: "k6-ecdsa-sign",
    account_description: "k6 ECDSA sign test account",
  });
  if (!assertOk("setup/newAccount", "POST /new_account", newAccountRes)) {
    throw new Error("setup failed: newAccount");
  }
  const { api_key } = newAccountRes.data as {
    api_key: string;
    wallet_address: string;
  };
  const authHeaders = { "X-Api-Key": api_key };

  const addUsageKeyRes = client.addUsageApiKey(
    {
      name: "k6-ecdsa-usage-key",
      description: "k6 ECDSA sign test usage key",
      can_create_groups: false,
      can_delete_groups: false,
      can_create_pkps: false,
      can_manage_ipfs_ids_in_groups: [],
      can_add_pkp_to_groups: [],
      can_remove_pkp_from_groups: [],
      can_execute_in_groups: [0],
    },
    authHeaders,
  );
  if (!assertOk("setup/addUsageApiKey", "POST /add_usage_api_key", addUsageKeyRes)) {
    throw new Error("setup failed: addUsageApiKey");
  }
  const usageApiKey = (addUsageKeyRes.data as { usage_api_key: string }).usage_api_key;

  return { usageKeyHeaders: { "X-Api-Key": usageApiKey } };
}

export default function (data: { usageKeyHeaders: { "X-Api-Key": string } }) {
  const client = new LitApiServerClient({ baseUrl: BASE_URL });

  const litActionRes = client.litAction(
    {
      code: ECDSA_SIGN_CODE,
      js_params: null,
    },
    data.usageKeyHeaders,
  );
  if (!assertOk("litAction/ecdsa-sign", "POST /lit_action", litActionRes)) return;

  const body = JSON.parse(litActionRes.response.body as string);
  checkAndLog(litActionRes.response, {
    "ecdsa-sign has no error": () => body.has_error === false,
  }, "litAction/ecdsa-sign");
  // TODO: verify signature (body.response.signature) for "Chipotle Rocks!" via ecrecover
}
