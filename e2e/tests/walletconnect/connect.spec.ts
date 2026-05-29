/**
 * ChainSecured login over WalletConnect.
 *
 * Uses the headless `@reown/walletkit` wallet from `fixtures/wc-wallet.ts` to
 * approve a WC v2 pairing with the dashboard. Same shape as the EOA connect
 * spec — verifies the pairing handshake completes for a wallet that isn't
 * yet registered as a ChainSecured account.
 *
 * Requires WC_PROJECT_ID in env (a free Reown project id; see e2e/README.md).
 */

import { test, expect } from '../../fixtures/test';

test.describe('ChainSecured login (WalletConnect)', () => {
  test('headless WC wallet pairs and surfaces "no account" for an unknown wallet', async ({
    dashboardPage,
    wcWallet,
  }) => {
    await dashboardPage.goto();
    // Start the pairing flow; pickWalletConnector('walletconnect') happens
    // inside startWalletLogin, after which the dashboard fires the
    // `lit:wc-display-uri` event with the pairing URI.
    const [uri] = await Promise.all([
      dashboardPage.waitForWcPairingUri(),
      dashboardPage.startWalletLogin('walletconnect'),
    ]);

    await wcWallet.pair(uri);

    await expect(dashboardPage.page.locator('#login-status')).toContainText(
      /No ChainSecured account found/i,
      { timeout: 45_000 },
    );
    await dashboardPage.expectLoggedOut();
  });
});
