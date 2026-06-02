/**
 * Encrypt/decrypt round-trip — parity with
 * `k6/correctness/lit-action-encrypt-decrypt.spec.ts`.
 *
 * Same shape as the k6 spec:
 *   setup: create account + usage key
 *   1. run an encrypt Lit Action with a random challenge against the account's PKP
 *   2. run a decrypt Lit Action over the ciphertext
 *   3. assert decrypted plaintext equals the original challenge
 *
 * The PKP id is the wallet_address returned by /new_account (matches how the
 * k6 setup wires PRECREATED_ACCOUNTS[i].walletAddress through to pkpId).
 */

import { test, expect } from '../../fixtures/test';
import { ENCRYPT_ACTION, DECRYPT_ACTION } from '../../fixtures/api-client';

test.describe('lit action — encrypt/decrypt', () => {
  test('encrypted ciphertext decrypts back to the original challenge', async ({
    apiClient,
    dashboardPage,
  }) => {
    const stamp = Date.now();
    const account = await apiClient.newAccount({
      account_name: `e2e-encdec-${stamp}`,
      account_description: 'e2e encrypt/decrypt test',
    });
    const { usage_api_key: usageApiKey } = await apiClient.addUsageApiKey(account.api_key, {
      name: `e2e-encdec-${stamp}-usage`,
      execute_in_groups: [0],
    });
    const pkpId = account.wallet_address;
    const challenge =
      Math.random().toString(36).slice(2) + Math.random().toString(36).slice(2);

    await dashboardPage.goto();
    await dashboardPage.loginWithApiKey(account.api_key);

    const encryptResult = await dashboardPage.runLitAction({
      usageApiKey,
      code: ENCRYPT_ACTION,
      jsParams: { pkpId, challenge },
    });
    expect(encryptResult.has_error).toBe(false);
    const ciphertext = encryptResult.response;
    expect(typeof ciphertext).toBe('string');
    expect((ciphertext as string).length).toBeGreaterThan(0);

    const decryptResult = await dashboardPage.runLitAction({
      usageApiKey,
      code: DECRYPT_ACTION,
      jsParams: { pkpId, ciphertext },
    });
    expect(decryptResult.has_error).toBe(false);
    expect(decryptResult.response).toBe(challenge);
  });
});
