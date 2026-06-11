/**
 * Regression test for the WalletConnect RPC-routing trap (eth_signTypedData_v4
 * → -32601 "Method not found").
 *
 * @walletconnect/ethereum-provider's Eip155Provider routes any method NOT in
 * the *approved* session namespace to the rpcMap HTTP node instead of the
 * wallet. The dashboard previously left eth_signTypedData_v4 as merely
 * *optional*; wallets that approve only the dapp's required methods (a common
 * real-world configuration) then dropped it from the namespace, so the
 * ChainSecured typed-data signature was sent to the public RPC — which has no
 * keys and answers -32601.
 *
 * Unlike the permissive `wcWallet` fixture (which echoes every supported
 * optional method back, masking the bug), this test pairs a wallet that
 * approves ONLY the required methods. The flow completes only because the
 * dashboard now declares the typed-data methods as required, keeping them in
 * the namespace so they route to the wallet.
 */

import { test, expect } from '../../fixtures/test';
import { createTestWcWallet, type TestWcWallet } from '../../fixtures/wc-wallet';

test.describe('ChainSecured typed-data signing over a restrictive WC wallet', () => {
  let restrictiveWallet: TestWcWallet;

  test.beforeEach(async () => {
    // Approve only the dapp's required methods — drop everything optional.
    restrictiveWallet = await createTestWcWallet({ approveOnlyRequired: true });
  });

  test.afterEach(async () => {
    await restrictiveWallet?.disconnectAll();
  });

  test('eth_signTypedData_v4 routes to the wallet, not the public RPC', async ({
    dashboardPage,
  }) => {
    const stamp = Date.now();
    await dashboardPage.goto();

    const [uri] = await Promise.all([
      dashboardPage.waitForWcPairingUri(),
      dashboardPage.startWalletCreate('walletconnect', {
        name: `e2e-cs-wc-restrictive-${stamp}`,
        description:
          'created from e2e/walletconnect/restrictive-namespace-signtypeddata.spec.ts',
      }),
    ]);
    await restrictiveWallet.pair(uri);

    const previewConfirm = dashboardPage.page.locator('#tx-preview-confirm');
    await expect(previewConfirm).toBeVisible({ timeout: 45_000 });
    await previewConfirm.click();

    // create_wallet_with_signature signs eth_signTypedData_v4 before the
    // on-chain write. If v4 routed to the RPC node this banner never appears —
    // the sign would reject with -32601 and the flow would abort.
    const banner = dashboardPage.page.locator('#new-account-banner');
    await expect(banner).toBeVisible({ timeout: 90_000 });
    await expect(banner).toContainText(/0x[0-9a-fA-F]{40}/);

    // The banner alone is NOT sufficient proof: the e2e fallback RPC is Anvil,
    // which signs eth_signTypedData_v4 with its unlocked deterministic accounts.
    // A misrouted signature would still come back valid and the banner would
    // still render — masking the routing bug. Assert the WALLET itself handled
    // the v4 request, which only happens when it's in the approved namespace.
    expect(restrictiveWallet.handledMethods).toContain('eth_signTypedData_v4');

    await dashboardPage.expectLoggedIn();
  });
});
