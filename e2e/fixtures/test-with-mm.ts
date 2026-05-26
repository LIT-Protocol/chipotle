import { testWithSynpress } from '@synthetixio/synpress';
import { metaMaskFixtures } from '@synthetixio/synpress/playwright';
import basicSetup from '../wallet-setup/basic.setup';
import { anvil } from './anvil';
import { LitApiClient, apiClient as defaultApiClient } from './api-client';
import { DashboardPage } from './dashboard';
import { createTestWcWallet, type TestWcWallet } from './wc-wallet';
import type { Hex } from 'viem';

/**
 * Synpress-wrapped `test` for MetaMask (EOA) specs only. Importing this pulls
 * in the Synpress wallet-cache build, which is slow and incompatible with
 * pure-API tests — see `test.ts` for the plain variant used elsewhere.
 *
 * `metaMaskFixtures` provides `metamask: MetaMask`, `metamaskPage: Page`, and
 * `extensionId: string`. We extend it with the same domain fixtures the plain
 * `test.ts` exposes so EOA specs can share helpers with the rest of the suite.
 */

export const test = testWithSynpress(metaMaskFixtures(basicSetup)).extend<{
  apiClient: LitApiClient;
  dashboardPage: DashboardPage;
  wcWallet: TestWcWallet;
  anvilSnap: void;
}>({
  apiClient: async ({}, use) => {
    await use(defaultApiClient);
  },

  dashboardPage: async ({ page }, use) => {
    await use(new DashboardPage(page));
  },

  wcWallet: async ({}, use) => {
    const wallet = await createTestWcWallet();
    await use(wallet);
    await wallet.disconnectAll();
  },

  anvilSnap: [
    async ({}, use) => {
      const id: Hex = await anvil.snapshot();
      await use();
      await anvil.revert(id);
    },
    { auto: true },
  ],
});

export const expect = test.expect;
