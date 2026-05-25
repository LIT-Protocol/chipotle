/**
 * Onboarding parameterized over `walletKind` (see playwright.config.ts).
 *
 * Runs as `flows-api` and `flows-wc`. The EOA/MetaMask onboarding journey
 * lives in `tests/eoa/new-chainsecured-account.spec.ts` — keeping it separate
 * means this file doesn't have to pull in Synpress, so the API + WC flow
 * projects skip the wallet-cache build step entirely.
 */

import { test, expect } from '../../fixtures/test';
import { HELLO_WORLD_ACTION } from '../../fixtures/api-client';

test.describe('onboarding flow', () => {
  test('user can sign in and reach the dashboard', async ({
    walletKind,
    dashboardPage,
    apiClient,
    wcWallet,
  }) => {
    await dashboardPage.goto();

    if (walletKind === 'api') {
      const stamp = Date.now();
      const account = await apiClient.newAccount({
        account_name: `e2e-onboard-${stamp}`,
        account_description: 'onboarding flow (api)',
      });
      await dashboardPage.loginWithApiKey(account.api_key);
      const { usage_api_key: usageApiKey } = await apiClient.addUsageApiKey(account.api_key, {
        name: `e2e-onboard-${stamp}-usage`,
        execute_in_groups: [0],
      });
      const result = await dashboardPage.runLitAction({
        usageApiKey,
        code: HELLO_WORLD_ACTION,
      });
      expect(result.response).toBe('Hello World!');
      return;
    }

    if (walletKind === 'walletconnect') {
      const [uri] = await Promise.all([
        dashboardPage.waitForWcPairingUri(),
        dashboardPage.startWalletCreate('walletconnect', {
          name: `e2e-onboard-wc-${Date.now()}`,
        }),
      ]);
      await wcWallet.pair(uri);
      await dashboardPage.page.locator('#tx-preview-confirm').click();
      await expect(dashboardPage.page.locator('#new-account-banner')).toBeVisible({
        timeout: 90_000,
      });
      await dashboardPage.expectLoggedIn();
      return;
    }

    test.skip(true, `onboarding flow does not cover walletKind=${walletKind}`);
  });
});
