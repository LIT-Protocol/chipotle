/**
 * Onboarding flow — WalletConnect ChainSecured auth.
 *
 * Picked up only by the `flows-wc` project, so the API project doesn't spin
 * up the WalletConnect fixture (which also requires WC_PROJECT_ID).
 */

import { test, expect } from '../../fixtures/test';

test.describe('onboarding flow (walletconnect)', () => {
  test('user can connect a WC wallet and create a ChainSecured account', async ({
    dashboardPage,
    wcWallet,
  }) => {
    await dashboardPage.goto();

    const [uri] = await Promise.all([
      dashboardPage.waitForWcPairingUri(),
      dashboardPage.startWalletCreate('walletconnect', {
        name: `e2e-onboard-wc-${Date.now()}`,
      }),
    ]);
    await wcWallet.pair(uri);

    const previewConfirm = dashboardPage.page.locator('#tx-preview-confirm');
    await expect(previewConfirm).toBeVisible({ timeout: 45_000 });
    await previewConfirm.click();

    await expect(dashboardPage.page.locator('#new-account-banner')).toBeVisible({
      timeout: 90_000,
    });
    await dashboardPage.expectLoggedIn();
  });
});
