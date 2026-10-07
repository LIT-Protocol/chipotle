/**
 * Tests Lit.Actions.Encrypt() and Lit.Actions.Decrypt() — encrypts a random
 * challenge in one Lit Action and decrypts it in a second, asserting the
 * round-trip produces the original plaintext.
 *
 * Flow:
 *   Setup: use a seeded account and grant the exact action CIDs access to its PKP
 *   1. Run a Lit Action that encrypts a random challenge with the PKP's AES key
 *   2. Run a second Lit Action that decrypts the ciphertext and returns plaintext
 *   3. Assert decrypted plaintext === original challenge
 *
 * Usage:
 *   k6 run k6/correctness/lit-action-encrypt-decrypt.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/correctness/lit-action-encrypt-decrypt.spec.ts
 */
import { sleep } from "k6";
import { checkAndLog, warnOnHttpFailures } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { PRECREATED_ACCOUNTS } from "../setup.ts";
import { assertOk } from "../helpers.ts";
import { ENCRYPT_CODE, DECRYPT_CODE } from "../LitActionCode/index.ts";
import { BASE_URL, COMMON_PARAMS } from "../defaults.ts";
import { ensureAccountCredits } from "../stripe.ts";

export interface EncryptDecryptSetupData {
  usageApiKey: string;
  pkpId: string;
  accountApiKey: string;
  groupId: string;
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
  ensureAccountCredits(client, { "X-Api-Key": account.apiKey });

  const adminHeaders = { "X-Api-Key": account.apiKey };
  // A wildcard key permits execution, but PKP crypto operations still need a
  // group containing this wallet and both exact action CIDs.
  const groupRes = client.addGroup({
    group_name: `k6-encdec-${Date.now()}`,
    group_description: "Encrypt/decrypt test permissions",
    pkp_ids_permitted: [account.walletAddress],
    cid_hashes_permitted: [],
  }, adminHeaders);
  if (!assertOk("setup/addGroup", "POST /add_group", groupRes)) {
    throw new Error("setup failed: addGroup");
  }
  const groupId = (groupRes.data as { group_id: string }).group_id;
  const data = {
    usageApiKey: account.usageApiKey, pkpId: account.walletAddress,
    accountApiKey: account.apiKey, groupId,
  };
  try {
    for (const code of [ENCRYPT_CODE, DECRYPT_CODE]) {
      const cidRes = client.getLitActionIpfsId(code);
      if (!assertOk("setup/getCid", "POST /get_lit_action_ipfs_id", cidRes)) {
        throw new Error("setup failed: get action CID");
      }
      const cid = (cidRes.response.body as string).replace(/^"|"$/g, "").trim();
      const grant = client.addActionToGroup({
        group_id: Number(groupId), action_ipfs_cid: cid,
      }, adminHeaders);
      if (!assertOk("setup/grantAction", "POST /add_action_to_group", grant)) {
        throw new Error("setup failed: grant action");
      }
    }

    // Poll the real encryption/decryption paths until grants propagate. Do not
    // count an expected transient denial as a correctness-check failure.
    const deadline = Date.now() + 45_000;
    const headers = { "X-Api-Key": account.usageApiKey };
    while (Date.now() < deadline) {
      const encrypted = client.litAction({
        code: ENCRYPT_CODE,
        js_params: { pkpId: account.walletAddress, challenge: "permission readiness" },
      }, headers);
      if (encrypted.response.status === 200) {
        const body = JSON.parse(encrypted.response.body as string);
        if (!body.has_error && typeof body.response === "string") {
          const decrypted = client.litAction({
            code: DECRYPT_CODE,
            js_params: { pkpId: account.walletAddress, ciphertext: body.response },
          }, headers);
          if (decrypted.response.status === 200) {
            const result = JSON.parse(decrypted.response.body as string);
            if (!result.has_error && result.response === "permission readiness") return data;
          }
        }
      }
      sleep(1);
    }
    throw new Error("setup failed: wallet permissions did not become usable within 45s");
  } catch (error) {
    teardown(data);
    throw error;
  }
}

export const options = {
  setupTimeout: "120s",
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

export function teardown(data: EncryptDecryptSetupData) {
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  const removed = client.removeGroup({ group_id: data.groupId }, { "X-Api-Key": data.accountApiKey });
  assertOk("teardown/removeGroup", "POST /remove_group", removed);
}

export const handleSummary = warnOnHttpFailures;
