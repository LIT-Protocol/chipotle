/**
 * Tests Lit.Actions.Encrypt() and Lit.Actions.Decrypt() — encrypts a random
 * challenge in one Lit Action and decrypts it in a second, asserting the
 * round-trip produces the original plaintext.
 *
 * Flow:
 *   1. Create a fresh account (wallet_address from newAccount is the PKP id)
 *   2. Run a Lit Action that encrypts a random challenge with the PKP's AES key
 *   3. Run a second Lit Action that decrypts the ciphertext and returns plaintext
 *   4. Assert decrypted plaintext === original challenge
 *
 * Usage:
 *   k6 run k6/lit-action-encrypt-decrypt.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/lit-action-encrypt-decrypt.spec.ts
 */
import type { Response } from "k6/http";
import { checkAndLog } from "../check.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { ENCRYPT_CODE, DECRYPT_CODE } from "../LitActionCode/index.ts";

const BASE_URL =
  __ENV.BASE_URL ||
  "https://e364da71b0c9af3b9068daa6321edd6ee932aa89-8000.dstack-pha-prod5.phala.network/core/v1";

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

  // Random challenge — two Math.random() halves give ~22 chars of alphanumeric entropy.
  const challenge =
    Math.random().toString(36).slice(2) +
    Math.random().toString(36).slice(2);

  // ── 1. Create account ─────────────────────────────────────────────────────
  // newAccount returns the account master wallet address, which is already
  // registered on-chain and usable as a PKP id for encrypt/decrypt.
  const newAccountRes = client.newAccount({
    account_name: "k6-encrypt-decrypt",
    account_description: "k6 encrypt/decrypt test account",
  });
  if (!assertOk("newAccount", "POST /new_account", newAccountRes)) return;
  const { api_key, wallet_address: pkpId } = newAccountRes.data as {
    api_key: string;
    wallet_address: string;
  };
  const authHeaders = { "X-Api-Key": api_key };

  // ── 2. Create usage API key for lit action calls ──────────────────────────
  const addUsageKeyRes = client.addUsageApiKey(
    {
      name: "k6-encrypt-usage-key",
      description: "k6 encrypt/decrypt test usage key",
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

  // ── 3. Encrypt challenge ──────────────────────────────────────────────────
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

  // ── 4. Decrypt ciphertext ─────────────────────────────────────────────────
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
