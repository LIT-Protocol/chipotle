/**
 * Shared setup utilities for k6 tests.
 * createAccountAndUsageKey — creates a new account and usage API key for lit_action.
 */
import { LitApiServerClient } from "./litApiServer.ts";
import { assertOk } from "./helpers.ts";
import { BASE_URL, COMMON_PARAMS, K6_RUN_ID } from "./defaults.ts";
import { SharedArray } from "k6/data";
import { ensureAccountCredits } from "./stripe.ts";

export interface AccountAndUsageKey {
  apiKey: string;
  walletAddress: string;
  usageApiKey: string;
}

/**
 * Shape of a pre-created account record stored in the local JSON file.
 */
export interface PrecreatedAccount {
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

  // Ensure the account has credits BEFORE any billed management calls.
  // addUsageApiKey is guarded by BilledManagementApiKey ($0.01/call),
  // and new accounts start at $0 balance.
  if (!ensureAccountCredits(client, authHeaders)) {
    console.warn(`${prefix}topUp: failed to ensure account credits — tests requiring credits may fail`);
  }

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

/**
 * Shared pool of pre-created accounts, loaded from a JSON file on disk.
 *
 * The file is expected to be either:
 *   - an object with an `accounts` array, or
 *   - a bare array of account objects.
 *
 * You can override the path with ACCOUNTS_FILE; by default we look for
 * `./data/accounts.json` relative to the k6 project root.
 */
const ACCOUNTS_FILE = __ENV.K6_ACCOUNTS_FILE || "./data/accounts.json";

export const PRECREATED_ACCOUNTS = new SharedArray<PrecreatedAccount>(
  "precreated-accounts",
  () => {
    try {
      const raw = open(ACCOUNTS_FILE);
      const parsed = JSON.parse(raw as string);
      if (Array.isArray(parsed)) {
        return parsed as PrecreatedAccount[];
      }
      if (parsed && Array.isArray((parsed as any).accounts)) {
        return (parsed as any).accounts as PrecreatedAccount[];
      }
      throw new Error(
        `Invalid accounts file format in ${ACCOUNTS_FILE}; expected an array or { accounts: [...] }`,
      );
    } catch (e) {
      // If the file does not exist yet or is temporarily invalid (e.g. being
      // written by the seeding script), treat this as "no pre-created accounts".
      console.warn(
        `PRECREATED_ACCOUNTS: unable to load ${ACCOUNTS_FILE} (${String(
          (e as Error).message ?? e,
        )}); continuing with an empty pool`,
      );
      return [] as PrecreatedAccount[];
    }
  },
);

