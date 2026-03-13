/**
 * Smoke test - hits get_node_chain_config, creates an account, and runs a hello-world lit action.
 * Use: k6 run smoke.spec.ts
 */
import type { Response } from "k6/http";
import { checkAndLog } from "./check.ts";
import { LitApiServerClient } from "./litApiServer.ts";

const baseUrl =
  __ENV.BASE_URL ||
  "https://e364da71b0c9af3b9068daa6321edd6ee932aa89-8000.dstack-pha-prod5.phala.network/core/v1";
const client = new LitApiServerClient({ baseUrl });

const HELLO_WORLD_CODE = 'Lit.Actions.setResponse({response: "Hello World!"})';

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_reqs: ["count>=4"],
    http_req_failed: ["rate<0.1"],
    checks: ["rate==1"],
  },
};

export default function () {
  // 1. Public endpoint (no auth)
  const chainConfigRes = client.getNodeChainConfig();
  if (!assertOk("getNodeChainConfig", "GET /get_node_chain_config", chainConfigRes)) return;
  checkAndLog(chainConfigRes.response, {
    "has chain config": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        return body && typeof body.chain_id === "number";
      } catch {
        return false;
      }
    },
  }, "getNodeChainConfig");

  // 2. Create account
  const newAccountRes = client.newAccount({
    account_name: "k6-smoke-test",
    account_description: "k6 smoke test account",
  });
  if (!assertOk("newAccount", "POST /new_account", newAccountRes)) return;
  const apiKey = (newAccountRes.data as { api_key: string }).api_key;
  const authHeaders = { "X-Api-Key": apiKey };

  // 3. Create usage API key for lit action calls
  const addUsageKeyRes = client.addUsageApiKey(
    {
      name: "k6-smoke-usage-key",
      description: "k6 smoke test usage key",
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
  if (!assertOk("addUsageApiKey", "POST /add_usage_api_key", addUsageKeyRes)) return;
  const usageApiKey = (addUsageKeyRes.data as { usage_api_key: string }).usage_api_key;
  const usageKeyHeaders = { "X-Api-Key": usageApiKey };

  // 4. Run hello-world lit action
  const litActionRes = client.litAction(
    { code: HELLO_WORLD_CODE, js_params: null },
    usageKeyHeaders,
  );
  if (!assertOk("litAction", "POST /lit_action", litActionRes)) return;
  checkAndLog(litActionRes.response, {
    "litAction has no error": (r) => {
      try {
        return JSON.parse(r.body as string).has_error === false;
      } catch {
        return false;
      }
    },
    "litAction response is Hello World!": (r) => {
      try {
        return JSON.parse(r.body as string).response === "Hello World!";
      } catch {
        return false;
      }
    },
  }, "litAction");
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