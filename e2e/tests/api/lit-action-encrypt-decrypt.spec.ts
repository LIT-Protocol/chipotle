/**
 * Encrypt/decrypt round-trip — parity with
 * `k6/correctness/lit-action-encrypt-decrypt.spec.ts`.
 *
 * Same shape as the k6 spec:
 *   setup: create account + action/wallet permission group + wildcard usage key
 *   1. run an encrypt Lit Action with a random challenge against the account's PKP
 *   2. run a decrypt Lit Action over the ciphertext
 *   3. assert decrypted plaintext equals the original challenge
 *
 * The PKP id is the wallet_address returned by /new_account (matches how the
 * k6 setup wires PRECREATED_ACCOUNTS[i].walletAddress through to pkpId).
 */

import { test, expect } from '../../fixtures/test';
import { ENCRYPT_ACTION, DECRYPT_ACTION } from '../../fixtures/api-client';

// These tests create isolated accounts. Reverting Anvil between them would
// rewind on-chain API-payer nonces beneath the live server's nonce manager.
test.use({ anvilSnap: async ({}, use) => { await use(); } });

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
    // The dashboard trims source before execution; authorize those exact bytes.
    await apiClient.permitWalletActions(
      account.api_key, `e2e-encdec-${stamp}`, [account.wallet_address],
      [ENCRYPT_ACTION.trim(), DECRYPT_ACTION.trim()],
    );
    const { usage_api_key: usageApiKey } = await apiClient.addUsageApiKey(account.api_key, {
      name: `e2e-encdec-${stamp}-usage`,
      execute_in_groups: [0],
    });
    const pkpId = account.wallet_address;
    const challenge =
      Math.random().toString(36).slice(2) + Math.random().toString(36).slice(2);

    // Verify the actual PKP path as grants propagate, rather than sleeping.
    await expect(async () => {
      const ready = await apiClient.litAction(usageApiKey, {
        code: ENCRYPT_ACTION.trim(), js_params: { pkpId, challenge },
      });
      expect(ready.has_error).toBe(false);
      expect(ready.response).toEqual(expect.any(String));
    }).toPass({ timeout: 45_000, intervals: [500, 1000, 2000] });

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
    await dashboardPage.page.locator('#action-runner-usage-key').fill('');
    await dashboardPage.page.locator('#action-runner-output').scrollIntoViewIfNeeded();
  });

  test('listing a foreign wallet in an own group cannot release its key', async ({ apiClient, dashboardPage }) => {
    test.setTimeout(150_000);
    const stamp = Date.now();
    const victim = await apiClient.newAccount({ account_name: `e2e-victim-${stamp}` });
    const attacker = await apiClient.newAccount({ account_name: `e2e-attacker-${stamp}` });
    // Return only the address, never private key material, on the positive path.
    const privateKeyAction = `async function main({ pkpId }) {
      return new ethers.Wallet(await Lit.Actions.getPrivateKey({ pkpId })).address;
    }`;
    const codes = [ENCRYPT_ACTION.trim(), DECRYPT_ACTION.trim(), privateKeyAction];
    await apiClient.permitWalletActions(
      victim.api_key, `victim-${stamp}`, [victim.wallet_address], codes,
    );
    await apiClient.permitWalletActions(
      attacker.api_key, `attacker-${stamp}`,
      [attacker.wallet_address, victim.wallet_address], codes,
    );
    const victimKey = (await apiClient.addUsageApiKey(victim.api_key, {
      name: 'victim execution', execute_in_groups: [0],
    })).usage_api_key;
    const attackerKey = (await apiClient.addUsageApiKey(attacker.api_key, {
      name: 'attacker execution', execute_in_groups: [0],
    })).usage_api_key;
    const challenge = `foreign-wallet-${stamp}`;
    let victimCiphertext: unknown;
    // Positive controls exercise all operations with the same group/key setup.
    // Poll until the grants are usable, avoiding false denials from RPC lag.
    for (const [account, key] of [[victim, victimKey], [attacker, attackerKey]] as const) {
      await expect(async () => {
        const encrypted = await apiClient.litAction(key, {
          code: ENCRYPT_ACTION.trim(), js_params: { pkpId: account.wallet_address, challenge },
        });
        expect(encrypted.has_error).toBe(false);
        expect(encrypted.response).toEqual(expect.any(String));
        const decrypted = await apiClient.litAction(key, {
          code: DECRYPT_ACTION.trim(),
          js_params: { pkpId: account.wallet_address, ciphertext: encrypted.response },
        });
        expect(decrypted.has_error).toBe(false);
        expect(decrypted.response).toBe(challenge);
        const address = await apiClient.litAction(key, {
          code: privateKeyAction, js_params: { pkpId: account.wallet_address },
        });
        expect(address.has_error).toBe(false);
        expect(String(address.response).toLowerCase()).toBe(account.wallet_address.toLowerCase());
        if (account === victim) victimCiphertext = encrypted.response;
      }).toPass({ timeout: 45_000, intervals: [500, 1000, 2000] });
    }

    for (const code of codes) {
      // Require the key-resolution error, not a generic failure or group denial:
      // the foreign address is explicitly permitted but absent from this account's
      // derivations. Deriving the unregistered path must fail the address check.
      await expect(apiClient.litAction(attackerKey, {
        code,
        js_params: { pkpId: victim.wallet_address, challenge, ciphertext: victimCiphertext },
      })).rejects.toThrow(/derivation path does not match pkpId/);
    }

    // Exercise the same denial in the dashboard and retain visual evidence.
    await dashboardPage.goto();
    await dashboardPage.loginWithApiKey(attacker.api_key);
    const deniedResponse = dashboardPage.page.waitForResponse(response =>
      response.url().endsWith('/core/v1/lit_action') && response.request().method() === 'POST',
    );
    // The current API reports key-resolution failures as 500, so the dashboard
    // shows its generic server error. Verify the actual denial in the response.
    await expect(dashboardPage.runLitAction({
      usageApiKey: attackerKey, code: ENCRYPT_ACTION.trim(),
      jsParams: { pkpId: victim.wallet_address, challenge },
    })).rejects.toThrow('Something went wrong on the server');
    const denied = await deniedResponse;
    expect(denied.status()).toBe(500);
    expect(await denied.text()).toContain('derivation path does not match pkpId');
    await dashboardPage.page.locator('#action-runner-usage-key').fill('');
    await dashboardPage.page.locator('#action-runner-output').scrollIntoViewIfNeeded();
  });
});
