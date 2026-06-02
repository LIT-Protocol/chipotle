/**
 * ChainSecured account creation over WalletConnect.
 *
 * Mirrors `tests/eoa/new-chainsecured-account.spec.ts` but uses a headless
 * WC v2 wallet via @reown/walletkit. The headless wallet auto-approves
 * sessions and signs all eth_* requests against Anvil, so the only DOM
 * action we drive is the dashboard's preview-and-confirm modal.
 */

import { test, expect } from '../../fixtures/test';

test.describe('ChainSecured account creation (WalletConnect)', () => {
  test('pair + sign newChainSecuredAccount through a headless WC wallet', async ({
    dashboardPage,
    wcWallet,
  }) => {
    const stamp = Date.now();
    await dashboardPage.goto();

    const [uri] = await Promise.all([
      dashboardPage.waitForWcPairingUri(),
      dashboardPage.startWalletCreate('walletconnect', {
        name: `e2e-cs-wc-${stamp}`,
        description: 'created from e2e/walletconnect/new-chainsecured-account.spec.ts',
      }),
    ]);
    await wcWallet.pair(uri);

    const previewConfirm = dashboardPage.page.locator('#tx-preview-confirm');
    await expect(previewConfirm).toBeVisible({ timeout: 45_000 });
    await previewConfirm.click();

    // The headless wallet auto-approves eth_sendTransaction — no extra UI.
    const banner = dashboardPage.page.locator('#new-account-banner');
    await expect(banner).toBeVisible({ timeout: 90_000 });
    await expect(banner).toContainText(/0x[0-9a-fA-F]{40}/);

    await dashboardPage.expectLoggedIn();
  });
});
