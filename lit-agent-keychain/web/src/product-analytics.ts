import { createAnalytics } from "./analytics.js";
import { digest } from "../../protocol/crypto.ts";
import type { Owner } from "../../protocol/schema.ts";

export const analytics = createAnalytics("keychain");

export function trackKeychainSession(
  owner: Owner,
  vaultId: string,
  resumed = false,
) {
  // Use the credential that actually signed in, not the vault's root owner:
  // recovery credentials can belong to different people.
  const id =
    owner.kind === "wallet"
      ? `wallet:${owner.address.toLowerCase()}`
      : `keychain-owner:${digest(owner)}`;
  analytics.identify(id);
  analytics.group(`keychain:${vaultId}`);
  analytics.capture(resumed ? "session_resumed" : "login_completed", {
    method: owner.kind,
  });
}

export const OPERATIONS: Record<string, string> = {
  "Verifying owner authorization…": "login",
  "Creating a passkey…": "passkey_create",
  "Finding your passkey…": "passkey_login",
  "Encrypting and approving secret…": "secret_create",
  "Deleting secret…": "secret_delete",
  "Encrypting a new version…": "secret_rotate",
  "Approving agent…": "agent_approve",
  "Approving agents…": "agent_approve",
  "Revoking agent…": "agent_revoke",
  "Updating policy…": "policy_update",
  "Renewing permissions…": "policy_renew",
  "Opening secure checkout…": "checkout",
  "Opening billing management…": "billing_portal",
};
