/**
 * ChainSecured account creation through MetaMask (EOA path).
 *
 * Drives the full "New User / ChainSecured mode" flow:
 *   1. Fill name + description, click "Connect wallet & create".
 *   2. Pick MetaMask in the dashboard's wallet picker.
 *   3. Approve the MetaMask connection.
 *   4. Click "Confirm & sign in wallet" in the dashboard's preview modal.
 *   5. Approve the on-chain `newChainSecuredAccount` transaction in MetaMask.
 *   6. Verify the success banner appears with the wallet address.
 *
 * The Anvil snapshot fixture wrapping every test means this leaves no chain
 * residue between specs.
 */

import { test, expect } from '../../fixtures/test-with-mm';

test.describe('ChainSecured account creation (EOA)', () => {
  test('create + sign newChainSecuredAccount with MetaMask', async ({
    dashboardPage,
    metamask,
  }) => {
    const stamp = Date.now();
    await dashboardPage.goto();
    await dashboardPage.startWalletCreate('metamask', {
      name: `e2e-cs-${stamp}`,
      description: 'created from e2e/new-chainsecured-account.spec.ts',
    });

    await metamask.connectToDapp();

    // Dashboard renders the calldata preview before popping MetaMask for the
    // actual signature. Approve it.
    const previewConfirm = dashboardPage.page.locator('#tx-preview-confirm');
    await expect(previewConfirm).toBeVisible({ timeout: 30_000 });
    await previewConfirm.click();

    // Now MetaMask asks the user to sign the transaction.
    await metamask.confirmTransaction();

    // After the tx confirms the dashboard shows the account banner with the
    // connected wallet address.
    const banner = dashboardPage.page.locator('#new-account-banner');
    await expect(banner).toBeVisible({ timeout: 60_000 });
    await expect(banner).toContainText(/0x[0-9a-fA-F]{40}/);

    await dashboardPage.expectLoggedIn();
  });
});
