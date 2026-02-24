/**
 * Tests Lit.Actions.verifyActionSignature() — validates that a signature
 * originated from a specific Lit Action, enabling verifiable computation.
 *
 * This script runs a two-step flow:
 *   1. signAsAction  — produce a signature using the action's own CID identity
 *   2. verifyActionSignature — confirm the signature came from that CID
 *
 * Usage:
 *   LIT_API_KEY=your-key k6 run k6/lit-action-verify-action-signature.spec.ts
 *   BASE_URL=https://your-instance/core/v1 LIT_API_KEY=your-key k6 run k6/lit-action-verify-action-signature.spec.ts
 *
 * Ref: https://datil.developer.litprotocol.com/sdk/serverless-signing/sign-as-action
 */
import { check } from "k6";
import type { Response } from "k6/http";
import { LitApiServerClient } from "./litApiServer.ts";

const BASE_URL =
  __ENV.BASE_URL ||
  "https://36da669c852c9bd4fdea27dd331c07ff776bd125-8000.dstack-pha-prod5.phala.network/core/v1";
const LIT_API_KEY = __ENV.LIT_API_KEY || "";

// keccak256("hello world") as a 32-byte array
const TO_SIGN = [
  71, 23, 50, 133, 168, 215, 52, 30, 94, 151, 47, 198, 119, 40, 99, 132, 248,
  2, 248, 239, 66, 165, 236, 95, 3, 187, 250, 37, 76, 176, 31, 173,
];

// Step 1 code: sign data with the action's own CID identity.
// Mirrors: const signAsActionCode = `(${_signAsActionLitAction.toString()})();`
const SIGN_AS_ACTION_CODE = `(async () => {
  const signature = await Lit.Actions.signAsAction({
    toSign,
    sigName,
    signingScheme,
  });
  Lit.Actions.setResponse({ response: JSON.stringify(signature) });
})();`;

// Step 2 code: verify that the signature came from the given action IPFS CID.
// Mirrors: const verifyActionCode = `(${_verifyActionSignatureLitAction.toString()})();`
const VERIFY_ACTION_CODE = `(async () => {
  const result = await Lit.Actions.verifyActionSignature({
    signingScheme,
    actionIpfsCid,
    toSign,
    signOutput,
  });
  Lit.Actions.setResponse({ response: JSON.stringify(result) });
})();`;

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_req_failed: ["rate<0.1"],
    http_req_duration: ["p(99)<30000"],
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
  check(response, {
    [`${name} 2xx`]: (r) =>
      (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  });
  return ok;
}

export default function () {
  if (!LIT_API_KEY) {
    console.error(
      "LIT_API_KEY is required. Set it via: LIT_API_KEY=your-key k6 run ...",
    );
    return;
  }

  const client = new LitApiServerClient({ baseUrl: BASE_URL });
  const authHeaders = { "X-Api-Key": LIT_API_KEY };

  // ── Step 1: resolve the IPFS CID for the sign action code ────────────────
  // The CID is the identity used by signAsAction — verifyActionSignature needs it.
  const ipfsRes = client.getLitActionIpfsId(
    encodeURIComponent(SIGN_AS_ACTION_CODE),
  );
  if (!assertOk("getLitActionIpfsId", "GET /get_lit_action_ipfs_id", ipfsRes))
    return;

  const actionIpfsCid = (ipfsRes.response.body as string)
    .replace(/^"|"$/g, "")
    .trim();
  check(ipfsRes.response, {
    "getLitActionIpfsId returns non-empty CID": () => actionIpfsCid.length > 0,
  });
  console.log(`actionIpfsCid: ${actionIpfsCid}`);

  // ── Step 2: sign data with the action's own CID identity ─────────────────
  const signRes = client.litAction(
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
  if (!assertOk("litAction/signAsAction", "POST /lit_action", signRes)) return;

  check(signRes.response, {
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
  });

  const signBody = JSON.parse(signRes.response.body as string);
  // signOutput is the signature object returned by Lit.Actions.signAsAction()
  // and JSON.stringified into the response field by the action code above.
  let signOutput: unknown;
  try {
    signOutput = JSON.parse(signBody.response);
  } catch {
    signOutput = signBody.response;
  }
  console.log(`signOutput: ${JSON.stringify(signOutput)}`);

  // ── Step 3: verify the signature came from the expected action CID ────────
  const verifyRes = client.litAction(
    {
      code: VERIFY_ACTION_CODE,
      js_params: {
        signingScheme: "EcdsaK256Sha256",
        actionIpfsCid,
        toSign: TO_SIGN,
        signOutput,
      },
    },
    authHeaders,
  );
  if (
    !assertOk("litAction/verifyActionSignature", "POST /lit_action", verifyRes)
  )
    return;

  check(verifyRes.response, {
    "verifyActionSignature has no error": (r) => {
      try {
        return JSON.parse(r.body as string).has_error === false;
      } catch {
        return false;
      }
    },
    "verifyActionSignature response is non-empty": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        return typeof body.response === "string" && body.response.length > 0;
      } catch {
        return false;
      }
    },
  });

  const verifyBody = JSON.parse(verifyRes.response.body as string);
  console.log(`verifyActionSignature response: ${verifyBody.response}`);
  console.log(`verifyActionSignature logs: ${verifyBody.logs}`);
}
