import { test as base } from '@playwright/test';
import { anvil } from './anvil';
import { LitApiClient, apiClient as defaultApiClient } from './api-client';
import { DashboardPage } from './dashboard';
import { createTestWcWallet, type TestWcWallet } from './wc-wallet';
import type { Hex } from 'viem';

/**
 * Default `test` for API-mode and WalletConnect specs. Synpress is NOT pulled
 * in here — Synpress's wallet-cache step wraps every test under its
 * `testWithSynpress(...)` chain (including pure-API ones) and that's both
 * slow and historically brittle against current MetaMask builds. EOA/MetaMask
 * tests use `test` from `test-with-mm.ts` instead, which is the
 * Synpress-wrapped variant.
 *
 * Both `test` exports share the same `dashboardPage`, `apiClient`, `wcWallet`,
 * and `anvilSnap` fixtures so specs are mode-agnostic where they can be.
 */

type WalletKind = 'api' | 'eoa' | 'walletconnect';

export const test = base.extend<{
  apiClient: LitApiClient;
  dashboardPage: DashboardPage;
  wcWallet: TestWcWallet;
  anvilSnap: void;
  walletKind: WalletKind;
}>({
  walletKind: ['api', { option: true }],

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
