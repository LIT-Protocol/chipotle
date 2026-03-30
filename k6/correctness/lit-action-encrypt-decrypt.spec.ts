/**
 * Tests Lit.Actions.Encrypt() and Lit.Actions.Decrypt() — encrypts a random
 * challenge in one Lit Action and decrypts it in a second, asserting the
 * round-trip produces the original plaintext.
 *
 * Flow:
 *   Setup: create account and usage key (wallet_address is the PKP id)
 *   1. Run a Lit Action that encrypts a random challenge with the PKP's AES key
 *   2. Run a second Lit Action that decrypts the ciphertext and returns plaintext
 *   3. Assert decrypted plaintext === original challenge
 *
 * Usage:
 *   k6 run k6/correctness/lit-action-encrypt-decrypt.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/correctness/lit-action-encrypt-decrypt.spec.ts
 */
import { checkAndLog, warnOnHttpFailures } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { PRECREATED_ACCOUNTS } from "../setup.ts";
import { assertOk } from "../helpers.ts";
import { ENCRYPT_CODE, DECRYPT_CODE } from "../LitActionCode/index.ts";
import { BASE_URL, COMMON_PARAMS } from "../defaults.ts";
import { topUpAccount, isBillingEnabled } from "../stripe.ts";

export interface EncryptDecryptSetupData {
  usageApiKey: string;
  pkpId: string;
}

export function setup(): EncryptDecryptSetupData {
  if (PRECREATED_ACCOUNTS.length === 0) {
    throw new Error(
      "No pre-created accounts found. Run accounts.seed.spec.ts first to generate k6/data/accounts.json",
    );
  }
  const account =
    PRECREATED_ACCOUNTS[Math.floor(Math.random() * PRECREATED_ACCOUNTS.length)];

  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  if (isBillingEnabled(client)) {
    topUpAccount(client, { "X-Api-Key": account.apiKey });
  }

  return { usageApiKey: account.usageApiKey, pkpId: account.walletAddress };
}

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_req_duration: ["p(99)<30000"],
    http_reqs: ["count>=1"],
    checks: ["rate==1"],
  },
};

export default function (data: EncryptDecryptSetupData) {
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  const { usageApiKey, pkpId } = data;
  const usageKeyHeaders = { "X-Api-Key": usageApiKey };

  // Random challenge — two Math.random() halves give ~22 chars of alphanumeric entropy.
  const challenge =
    Math.random().toString(36).slice(2) +
    Math.random().toString(36).slice(2);

  // ── 1. Encrypt challenge ──────────────────────────────────────────────────
  const encryptRes = client.litAction(
    {
      code: ENCRYPT_CODE,
      js_params: { pkpId, challenge },
    },
    usageKeyHeaders,
  );
  if (!assertOk("litAction/encrypt", "POST /lit_action", encryptRes)) return;

  const encryptBody = JSON.parse(encryptRes.response.body as string);
  checkAndLog(encryptRes.response, {
    "encrypt has no error": () => encryptBody.has_error === false,
    "encrypt returns non-empty ciphertext": () =>
      typeof encryptBody.response === "string" && encryptBody.response.length > 0,
  }, "litAction/encrypt");

  if (encryptBody.has_error || typeof encryptBody.response !== "string") {
    console.error(`encrypt failed — logs: ${encryptBody.logs}`);
    return;
  }
  const ciphertext: string = encryptBody.response;

  // ── 2. Decrypt ciphertext ─────────────────────────────────────────────────
  const decryptRes = client.litAction(
    {
      code: DECRYPT_CODE,
      js_params: { pkpId, ciphertext },
    },
    usageKeyHeaders,
  );
  if (!assertOk("litAction/decrypt", "POST /lit_action", decryptRes)) return;

  const decryptBody = JSON.parse(decryptRes.response.body as string);
  checkAndLog(decryptRes.response, {
    "decrypt has no error": () => decryptBody.has_error === false,
    "decrypted plaintext matches challenge": () =>
      decryptBody.response === challenge,
  }, "litAction/decrypt");
}

export const handleSummary = warnOnHttpFailures;
