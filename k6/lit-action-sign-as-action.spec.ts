/**
 * Tests Lit.Actions.signAsAction() — signs data using the action's own
 * cryptographic identity derived from its IPFS CID.
 *
 * Usage:
 *   k6 run k6/lit-action-sign-as-action.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/lit-action-sign-as-action.spec.ts
 *
 * Ref: https://datil.developer.litprotocol.com/sdk/serverless-signing/sign-as-action
 */
import type { Response } from "k6/http";
import { checkAndLog } from "./check.ts";
import { LitApiServerClient } from "./litApiServer.ts";

const BASE_URL =
  __ENV.BASE_URL ||
  "https://e364da71b0c9af3b9068daa6321edd6ee932aa89-8000.dstack-pha-prod5.phala.network/core/v1";

// keccak256("hello world") as a 32-byte array
const TO_SIGN = [
  71, 23, 50, 133, 168, 215, 52, 30, 94, 151, 47, 198, 119, 40, 99, 132, 248,
  2, 248, 239, 66, 165, 236, 95, 3, 187, 250, 37, 76, 176, 31, 173,
];

// Mirrors the sample from the Lit docs:
//   const signAsActionCode = `(${_signAsActionLitAction.toString()})();`
const SIGN_AS_ACTION_CODE = `(async () => {
  const signature = await Lit.Actions.signAsAction({
    toSign,
    sigName,
    signingScheme,
  });
  Lit.Actions.setResponse({ response: JSON.stringify(signature) });
})();`;

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

export default function () {
  const client = new LitApiServerClient({ baseUrl: BASE_URL });

  const newAccountRes = client.newAccount({
    account_name: "k6-lit-action-sign-as-action",
    account_description: "k6 test account",
    initial_balance: "10000",
  });
  if (!assertOk("newAccount", "POST /new_account", newAccountRes)) return;
  const apiKey = (newAccountRes.data as { api_key: string }).api_key;
  const authHeaders = { "X-Api-Key": apiKey };

  const res = client.litAction(
    {
      code: SIGN_AS_ACTION_CODE,
      js_params: {
        toSign: TO_SIGN,
        sigName: "sig",
        signingScheme: "EcdsaK256Sha256",
      },
    },
    authHeaders,
  );

  if (!assertOk("litAction/signAsAction", "POST /lit_action", res)) return;

  checkAndLog(res.response, {
    "signAsAction has no error": (r) => {
      try {
        return JSON.parse(r.body as string).has_error === false;
      } catch {
        return false;
      }
    },
    "signAsAction response is non-empty": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        return typeof body.response === "string" && body.response.length > 0;
      } catch {
        return false;
      }
    },
    "signAsAction logs are present": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        return typeof body.logs === "string";
      } catch {
        return false;
      }
    },
  }, "litAction/signAsAction");

  const body = JSON.parse(res.response.body as string);
  console.log(`signAsAction response: ${body.response}`);
  console.log(`signAsAction logs: ${body.logs}`);
}
