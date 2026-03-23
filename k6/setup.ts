/**
 * Shared setup utilities for k6 tests.
 * createAccountAndUsageKey — creates a new account and usage API key for lit_action.
 */
import { LitApiServerClient } from "./litApiServer.ts";
import { assertOk } from "./helpers.ts";
import { BASE_URL, COMMON_PARAMS, K6_RUN_ID } from "./defaults.ts";

export interface AccountAndUsageKey {
  apiKey: string;
  walletAddress: string;
  usageApiKey: string;
}

/**
 * Create a new account and usage API key. Throws on failure.
 * Lit actions require a usage key with can_execute_in_groups; the account key cannot run actions.
 */
export function createAccountAndUsageKey(options: {
  baseUrl?: string;
  accountName: string;
  accountDescription: string;
  usageKeyName: string;
  usageKeyDescription: string;
  setupContext?: string;
}): AccountAndUsageKey {
  console.log(`k6 run correlation id: ${K6_RUN_ID}`);
  const client = new LitApiServerClient({
    baseUrl: options.baseUrl ?? BASE_URL,
    commonRequestParameters: COMMON_PARAMS,
  });
  const prefix = options.setupContext ? `${options.setupContext}/` : "setup/";

  const newAccountRes = client.newAccount({
    account_name: options.accountName,
    account_description: options.accountDescription,
  });
  if (!assertOk(`${prefix}newAccount`, "POST /new_account", newAccountRes)) {
    throw new Error("setup failed: newAccount");
  }
  const { api_key: apiKey, wallet_address: walletAddress } = newAccountRes.data as {
    api_key: string;
    wallet_address: string;
  };
  const authHeaders = { "X-Api-Key": apiKey };

  const addUsageKeyRes = client.addUsageApiKey(
    {
      name: options.usageKeyName,
      description: options.usageKeyDescription,
      can_create_groups: false,
      can_delete_groups: false,
      can_create_pkps: false,
      manage_ipfs_ids_in_groups: [],
      add_pkp_to_groups: [],
      remove_pkp_from_groups: [],
      execute_in_groups: [0],
    },
    authHeaders,
  );
  if (!assertOk(`${prefix}addUsageApiKey`, "POST /add_usage_api_key", addUsageKeyRes)) {
    throw new Error("setup failed: addUsageApiKey");
  }
  const usageApiKey = (addUsageKeyRes.data as { usage_api_key: string }).usage_api_key;

  return { apiKey, walletAddress, usageApiKey };
}
