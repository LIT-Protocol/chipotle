/**
 * ChainSecured login over an EIP-1193 injected wallet (MetaMask via Synpress).
 *
 * The existing-user wallet path needs a pre-registered ChainSecured account.
 * On a freshly-booted local environment that doesn't exist yet, so this spec
 * focuses on the "connect happens cleanly" portion:
 *
 *   1. Existing User → ChainSecured mode → "Connect wallet".
 *   2. Wallet picker shows; choose MetaMask.
 *   3. MetaMask prompts for connection; approve.
 *   4. The dashboard renders an error in `#login-status` along the lines of
 *      "No ChainSecured account found for this wallet" (which is the expected
 *      outcome for an unregistered Anvil EOA).
 *
 * A separate spec (`new-chainsecured-account.spec.ts`) drives the full
 * registration write.
 */

import { test, expect } from '../../fixtures/test-with-mm';

test.describe('ChainSecured login (EOA)', () => {
  test('connect prompts MetaMask and surfaces "no account" for a fresh wallet', async ({
    dashboardPage,
    metamask,
  }) => {
    await dashboardPage.goto();
    await dashboardPage.startWalletLogin('metamask');
    // Synpress drives the MetaMask "Connect" approval popup.
    await metamask.connectToDapp();

    await expect(dashboardPage.page.locator('#login-status')).toContainText(
      /No ChainSecured account found/i,
      { timeout: 30_000 },
    );
    // Dashboard stays on the login page in this failure case.
    await dashboardPage.expectLoggedOut();
  });
});
