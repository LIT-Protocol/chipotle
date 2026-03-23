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
import { checkAndLog } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { createAccountAndUsageKey } from "../setup.ts";
import { assertOk } from "../helpers.ts";
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

export function setup() {
    const { usageApiKey } = createAccountAndUsageKey({
        accountName: "k6-ecdsa-sign",
        accountDescription: "k6 ECDSA sign test account",
        usageKeyName: "k6-ecdsa-usage-key",
        usageKeyDescription: "k6 ECDSA sign test usage key",
        setupContext: "ecdsa-sign",
    });
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
