/**
 * Smoke test - hits get_node_chain_config, creates an account, and runs a hello-world lit action.
 * Use: k6 run smoke.spec.ts
 */
import { checkAndLog } from "./helpers.ts";
import { LitApiServerClient } from "./litApiServer.ts";
import { PRECREATED_ACCOUNTS } from "./setup.ts";
import { assertOk } from "./helpers.ts";
import { HELLO_WORLD_CODE } from "./LitActionCode/index.ts";
import { BASE_URL, COMMON_PARAMS } from "./defaults.ts";

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_reqs: ["count>=1"],
    http_req_failed: ["rate<0.1"],
    checks: ["rate==1"],
  },
};

export interface SmokeSetupData {
  usageApiKey: string;
}

export function setup(): SmokeSetupData {
  if (PRECREATED_ACCOUNTS.length === 0) {
    throw new Error(
      "No pre-created accounts found. Run accounts.seed.spec.ts first to generate k6/data/accounts.json",
    );
  }
  const account =
    PRECREATED_ACCOUNTS[Math.floor(Math.random() * PRECREATED_ACCOUNTS.length)];
  return { usageApiKey: account.usageApiKey };
}

export default function (data: SmokeSetupData) {
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  const usageKeyHeaders = { "X-Api-Key": data.usageApiKey };

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

  // 2. Run hello-world lit action
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