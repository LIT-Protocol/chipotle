/**
 * Integration tests for lit-api-server.
 * Exercises every endpoint in the OpenAPI spec using a full account lifecycle.
 *
 * Usage:
 *   k6 run k6/integration.spec.ts
 *   BASE_URL=https://your-instance.phala.network/core/v1 k6 run k6/integration.spec.ts
 */
import { check } from "k6";
import type { Response } from "k6/http";
import { LitApiServerClient } from "./litApiServer.ts";

const BASE_URL =
  __ENV.BASE_URL ||
  "https://36da669c852c9bd4fdea27dd331c07ff776bd125-8000.dstack-pha-prod5.phala.network/core/v1";

// Pre-existing account API key. When set, newAccount is skipped.
// Required when the deployment contract is not configured for new account creation.
// See BUGS.md BUG-001.
const LIT_API_KEY = __ENV.LIT_API_KEY || "";

const HELLO_WORLD_CODE = 'Lit.Actions.setResponse({response: "Hello World!"})';

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    // newAccount may 500 when contract is not configured (BUG-001); allow up to 50% failure
    // rate so the suite can still run with LIT_API_KEY fallback.
    http_req_failed: ["rate<0.5"],
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
  const client = new LitApiServerClient({ baseUrl: BASE_URL });

  // ── 1. getNodeChainConfig ─────────────────────────────────────────────────
  const chainConfigRes = client.getNodeChainConfig();
  if (!assertOk("getNodeChainConfig", "GET /get_node_chain_config", chainConfigRes)) return;
  check(chainConfigRes.response, {
    "chain config has numeric chain_id": (r) => {
      try {
        return typeof JSON.parse(r.body as string).chain_id === "number";
      } catch {
        return false;
      }
    },
  });

  // ── 2. getLitActionIpfsId ─────────────────────────────────────────────────
  const ipfsRes = client.getLitActionIpfsId(encodeURIComponent(HELLO_WORLD_CODE));
  if (!assertOk("getLitActionIpfsId", "GET /get_lit_action_ipfs_id/{code}", ipfsRes)) return;
  const ipfsId = (ipfsRes.response.body as string).replace(/^"|"$/g, "").trim();
  check(ipfsRes.response, {
    "ipfs id is non-empty string": () => ipfsId.length > 0,
  });

  // ── 3. newAccount ─────────────────────────────────────────────────────────
  // BUG-001: May 500 when the AccountConfig contract is not configured on this
  // deployment. Fall back to LIT_API_KEY env var when that happens.
  let apiKey = LIT_API_KEY;
  if (!apiKey) {
    const newAccountRes = client.newAccount({
      account_name: "k6-integration-test",
      account_description: "Integration test account",
    });
    if (!assertOk("newAccount", "POST /new_account", newAccountRes)) {
      console.warn("newAccount failed — set LIT_API_KEY to test authenticated endpoints (see BUGS.md BUG-001)");
      return;
    }
    const newAccountData = newAccountRes.data as { api_key: string; wallet_address: string };
    check(newAccountRes.response, {
      "newAccount returns api_key": () =>
        typeof newAccountData.api_key === "string" && newAccountData.api_key.length > 0,
      "newAccount returns wallet_address": () =>
        typeof newAccountData.wallet_address === "string" && newAccountData.wallet_address.length > 0,
    });
    apiKey = newAccountData.api_key;
  }
  const authHeaders = { "X-Api-Key": apiKey };

  // ── 4. accountExists ──────────────────────────────────────────────────────
  const existsRes = client.accountExists(authHeaders);
  if (!assertOk("accountExists", "GET /account_exists", existsRes)) return;
  check(existsRes.response, {
    "accountExists returns true": (r) => {
      try {
        return JSON.parse(r.body as string) === true;
      } catch {
        return false;
      }
    },
  });

  // ── 5. createWallet ───────────────────────────────────────────────────────
  const createWalletRes = client.createWallet(authHeaders);
  if (!assertOk("createWallet", "GET /create_wallet", createWalletRes)) return;
  const walletData = createWalletRes.data as { wallet_address: string };
  check(createWalletRes.response, {
    "createWallet returns wallet_address": () =>
      typeof walletData.wallet_address === "string" && walletData.wallet_address.length > 0,
  });
  const walletAddress = walletData.wallet_address;

  // ── 6. listWallets ────────────────────────────────────────────────────────
  const listWalletsRes = client.listWallets(
    { page_number: "0", page_size: "10" },
    authHeaders,
  );
  if (!assertOk("listWallets", "GET /list_wallets", listWalletsRes)) return;
  check(listWalletsRes.response, {
    "listWallets returns array": (r) => {
      try {
        return Array.isArray(JSON.parse(r.body as string));
      } catch {
        return false;
      }
    },
  });

  // ── 7. addGroup ───────────────────────────────────────────────────────────
  const addGroupRes = client.addGroup(
    {
      group_name: "k6-test-group",
      group_description: "Integration test group",
      permitted_actions: [],
      pkps: [],
      all_wallets_permitted: true,
      all_actions_permitted: true,
    },
    authHeaders,
  );
  if (!assertOk("addGroup", "POST /add_group", addGroupRes)) return;
  check(addGroupRes.response, {
    "addGroup success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  });

  // ── 8. listGroups — extract groupId for subsequent tests ──────────────────
  const listGroupsRes = client.listGroups(
    { page_number: "0", page_size: "10" },
    authHeaders,
  );
  if (!assertOk("listGroups", "GET /list_groups", listGroupsRes)) return;
  check(listGroupsRes.response, {
    "listGroups returns array": (r) => {
      try {
        return Array.isArray(JSON.parse(r.body as string));
      } catch {
        return false;
      }
    },
  });
  const groups = listGroupsRes.data as Array<{ id: string; name: string; description: string }>;
  if (!groups || groups.length === 0) {
    console.error("listGroups returned empty array after addGroup");
    return;
  }
  const groupId = groups[groups.length - 1].id; // use the most recently created group

  // ── 9. addActionToGroup ───────────────────────────────────────────────────
  const addActionRes = client.addActionToGroup(
    {
      group_id: groupId,
      action_ipfs_cid: ipfsId,
      name: "hello-world",
      description: "Hello World lit action",
    },
    authHeaders,
  );
  if (!assertOk("addActionToGroup", "POST /add_action_to_group", addActionRes)) return;
  check(addActionRes.response, {
    "addActionToGroup success": (r) => {
      try {
        return JSON.parse(r.body as string).success === true;
      } catch {
        return false;
      }
    },
  });

  // ── 10. listActions ───────────────────────────────────────────────────────
  const listActionsRes = client.listActions(
    { group_id: groupId, page_number: "0", page_size: "10" },
    authHeaders,
  );
  if (!assertOk("listActions", "GET /list_actions", listActionsRes)) return;
  check(listActionsRes.response, {
    "listActions returns array": (r) => {
      try {
        return Array.isArray(JSON.parse(r.body as string));
      } catch {
        return false;
      }
    },
  });
}
