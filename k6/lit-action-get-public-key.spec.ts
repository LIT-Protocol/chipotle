/**
 * Tests Lit.Actions.getActionPublicKey() — retrieves the public key and
 * derives the Ethereum address for a Lit Action from within the action itself.
 *
 * Usage:
 *   k6 run k6/lit-action-get-public-key.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/lit-action-get-public-key.spec.ts
 *
 * Ref: https://datil.developer.litprotocol.com/sdk/serverless-signing/sign-as-action
 */
import type { Response } from "k6/http";
import { checkAndLog } from "./check.ts";
import { LitApiServerClient } from "./litApiServer.ts";

const BASE_URL =
  __ENV.BASE_URL ||
  "https://e364da71b0c9af3b9068daa6321edd6ee932aa89-8000.dstack-pha-prod5.phala.network/core/v1";

// Taken verbatim from the Lit docs sample (standalone IIFE, no jsParams needed).
// Uses Lit.Auth.actionIpfsIdStack[0] to self-identify and ethers (pre-loaded
// in the Lit Actions runtime) to derive the Ethereum address.
const GET_PUBLIC_KEY_CODE = `(async () => {
  const actionIpfsCid = Lit.Auth.actionIpfsIdStack[0];
  const actionPublicKey = await Lit.Actions.getActionPublicKey({
    signingScheme: 'EcdsaK256Sha256',
    actionIpfsCid,
  });
  Lit.Actions.setResponse({
    response: JSON.stringify({
      actionPublicKey,
      actionAddress: ethers.utils.computeAddress(actionPublicKey),
      actionIpfsCid,
    }),
  });
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
    account_name: "k6-lit-action-get-public-key",
    account_description: "k6 test account",
    initial_balance: "10000",
  });
  if (!assertOk("newAccount", "POST /new_account", newAccountRes)) return;
  const apiKey = (newAccountRes.data as { api_key: string }).api_key;
  const authHeaders = { "X-Api-Key": apiKey };

  const res = client.litAction(
    {
      code: GET_PUBLIC_KEY_CODE,
      js_params: null,
    },
    authHeaders,
  );

  if (!assertOk("litAction/getActionPublicKey", "POST /lit_action", res))
    return;

  checkAndLog(res.response, {
    "getActionPublicKey has no error": (r) => {
      try {
        return JSON.parse(r.body as string).has_error === false;
      } catch {
        return false;
      }
    },
    "getActionPublicKey response contains actionPublicKey": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        const result = JSON.parse(body.response);
        return (
          typeof result.actionPublicKey === "string" &&
          result.actionPublicKey.length > 0
        );
      } catch {
        return false;
      }
    },
    "getActionPublicKey response contains actionAddress": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        const result = JSON.parse(body.response);
        // Ethereum addresses are 42-char hex strings starting with 0x
        return /^0x[0-9a-fA-F]{40}$/.test(result.actionAddress);
      } catch {
        return false;
      }
    },
    "getActionPublicKey response contains actionIpfsCid": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        const result = JSON.parse(body.response);
        return (
          typeof result.actionIpfsCid === "string" &&
          result.actionIpfsCid.length > 0
        );
      } catch {
        return false;
      }
    },
  }, "litAction/getActionPublicKey");

  const body = JSON.parse(res.response.body as string);
  const result = JSON.parse(body.response);
  console.log(`actionPublicKey: ${result.actionPublicKey}`);
  console.log(`actionAddress:   ${result.actionAddress}`);
  console.log(`actionIpfsCid:   ${result.actionIpfsCid}`);
}
