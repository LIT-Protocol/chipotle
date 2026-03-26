/**
 * Account seeding script for k6 tests.
 *
 * This script creates a configurable number of accounts and usage API keys
 * using the lit-api-server, then prints them as JSON to stdout so you can
 * redirect the output into a local file. Other k6 tests can then read from
 * that file instead of creating new accounts each run.
 *
 * Example:
 *   # Create 50 accounts and write them to k6/data/accounts.json
 *   ACCOUNTS_COUNT=50 k6 run k6/accounts.seed.spec.ts > k6/data/accounts.json
 *
 * Environment:
 *   ACCOUNTS_COUNT - Number of accounts to create (default: 40)
 */
import { sleep } from "k6";
import {
  createAccountAndUsageKey,
  type AccountAndUsageKey,
} from "./setup.ts";

interface SeedAccount extends AccountAndUsageKey {}

const ACCOUNTS_COUNT = parseInt(__ENV.ACCOUNTS_COUNT || "40", 10);
const ACCOUNTS_FILE = __ENV.ACCOUNTS_FILE || "./data/accounts.json";

// Allow enough time for creating many accounts during setup().
export const options = {
  setupTimeout: "30m",
};

export function setup(): SeedAccount[] {
  const count = Number.isFinite(ACCOUNTS_COUNT) && ACCOUNTS_COUNT > 0
    ? ACCOUNTS_COUNT
    : 40;

  // Load any existing accounts from disk so we can append instead of overwrite.
  let existingAccounts: SeedAccount[] = [];
  try {
    const rawExisting = open(ACCOUNTS_FILE);
    const parsedExisting = JSON.parse(rawExisting as string);
    if (Array.isArray(parsedExisting)) {
      existingAccounts = parsedExisting as SeedAccount[];
    } else if (parsedExisting && Array.isArray(parsedExisting.accounts)) {
      existingAccounts = parsedExisting.accounts as SeedAccount[];
    }
  } catch {
    // It's fine if the file does not exist yet; we'll treat this as empty.
    existingAccounts = [];
  }

  const accounts: SeedAccount[] = [];
  for (let i = 0; i < count; i++) {
    try {
      const { apiKey, walletAddress, usageApiKey } = createAccountAndUsageKey({
        accountName: `k6-seed-account-${i + 1}`,
        accountDescription: `k6 pre-created account ${i + 1}`,
        usageKeyName: `k6-seed-usage-key-${i + 1}`,
        usageKeyDescription: `k6 pre-created usage key ${i + 1}`,
        setupContext: "seed",
      });
      accounts.push({ apiKey, walletAddress, usageApiKey });
    } catch (e) {
      console.error(
        `accounts.seed: failed to create account ${i + 1} of ${count} (${String(
          (e as Error).message ?? e,
        )}); skipping`,
      );
      continue;
    }

    // Small delay between creations to avoid hammering the API too hard.
    sleep(0.1);
  }

  const allAccounts = existingAccounts.concat(accounts);

  const payload = {
    generated_at: new Date().toISOString(),
    count: allAccounts.length,
    accounts: allAccounts,
  };

  // Print JSON so it can be redirected to a file.
  console.log(JSON.stringify(payload, null, 2));

  return accounts;
}

export default function () {
  // No per-VU work; all the interesting work happens in setup().
}

