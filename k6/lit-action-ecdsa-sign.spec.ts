/**
 * Tests Lit.Actions.signEcdsa() — signs data using a PKP's ECDSA key.
 *
 * Flow:
 *   1. Create a fresh account and wallet (the wallet address IS the PKP ID )
 *   2. Run a Lit Action that calls signEcdsa with that PKP ID / wallet address
 *   3. Assert the response contains a non-empty hex signature with no error
 *
 * Usage:
 *   k6 run k6/lit-action-ecdsa-sign.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/lit-action-ecdsa-sign.spec.ts
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

// pkpId and sigName are injected via jsParams.
const ECDSA_SIGN_CODE = `(async () => {
  const sig = await Lit.Actions.signEcdsa({
    toSign,
    pkpId,
    sigName,
  });
  Lit.Actions.setResponse({ response: JSON.stringify(sig) });
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

  // ── 1. Create account ─────────────────────────────────────────────────────
  const newAccountRes = client.newAccount({
    account_name: "k6-lit-action-ecdsa-sign",
    account_description: "k6 test account",
  });
  if (!assertOk("newAccount", "POST /new_account", newAccountRes)) return;
  const apiKey = (newAccountRes.data as { api_key: string }).api_key;
  const authHeaders = { "X-Api-Key": apiKey };

  // ── 2. Create wallet ──────────────────────────────────────────────────────
  const walletRes = client.createWallet(authHeaders);
  if (!assertOk("createWallet", "GET /create_wallet", walletRes)) return;
  const walletAddress = (walletRes.data as { wallet_address: string })
    .wallet_address;
  checkAndLog(walletRes.response, {
    "createWallet returns wallet_address": () =>
      typeof walletAddress === "string" && walletAddress.length > 0,
  }, "createWallet");

  // ── 3. List wallets to get the PKP ID / wallet address ─────────────────────────────
  // createWallet returns only the Ethereum address; listWallets returns the
  // full WalletItem including the PKP ID / wallet address.
  const listRes = client.listWallets(
    { page_number: "0", page_size: "10" },
    authHeaders,
  );
  if (!assertOk("listWallets", "GET /list_wallets", listRes)) return;
  const wallets = listRes.data as Array<{
    wallet_address: string;    
  }>;
  const normalize = (addr: string) => addr.replace(/^0x/i, "").toLowerCase();
  const wallet = wallets.find(
    (w) => normalize(w.wallet_address) === normalize(walletAddress),
  );
  checkAndLog(listRes.response, {
    "listWallets finds created wallet": () => wallet !== undefined,
  }, "listWallets");
  if (!wallet) return;
  const pkpId = wallet.wallet_address;
  console.log(`pkpId: ${pkpId}`);

  // ── 4. Sign with signEcdsa ────────────────────────────────────────────────
  const res = client.litAction(
    {
      code: ECDSA_SIGN_CODE,
      js_params: {
        toSign: TO_SIGN,
        pkpId,
        sigName: "sig",
      },
    },
    authHeaders,
  );

  if (!assertOk("litAction/signEcdsa", "POST /lit_action", res)) return;

  checkAndLog(res.response, {
    "signEcdsa has no error": (r) => {
      try {
        return JSON.parse(r.body as string).has_error === false;
      } catch {
        return false;
      }
    },
    "signEcdsa response is non-empty": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        return typeof body.response === "string" && body.response.length > 0;
      } catch {
        return false;
      }
    },
    "signEcdsa logs are present": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        return typeof body.logs === "string";
      } catch {
        return false;
      }
    },
  }, "litAction/signEcdsa");

  const body = JSON.parse(res.response.body as string);
  console.log(`signEcdsa response: ${body.response}`);
  console.log(`signEcdsa logs: ${body.logs}`);
}
