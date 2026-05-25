/**
 * Smoke test — parity with `k6/smoke.spec.ts`.
 *
 * 1. Public endpoint `/get_node_chain_config` responds with a chain_id.
 * 2. The dashboard loads to the login page.
 * 3. A hello-world Lit Action returns "Hello World!" through the dashboard's
 *    Action Runner, exercising the full UI → lit-api-server → lit-actions path.
 */

import { test, expect } from '../../fixtures/test';
import { HELLO_WORLD_ACTION } from '../../fixtures/api-client';

test.describe('smoke', () => {
  test('public chain config endpoint returns a chain_id', async ({ apiClient }) => {
    const cfg = await apiClient.getNodeChainConfig();
    expect(typeof cfg.chain_id).toBe('number');
  });

  test('dashboard renders the login page on first visit', async ({ dashboardPage }) => {
    await dashboardPage.goto();
    await dashboardPage.expectLoggedOut();
    // Existing User / API mode card is the default-active tab.
    await expect(dashboardPage.page.locator('#btn-login')).toBeVisible();
    // New User tab houses the create-account button; verify the tab switches.
    await dashboardPage.showNewUserTab();
    await expect(dashboardPage.page.locator('#btn-create-account')).toBeVisible();
  });

  test('hello world Lit Action runs end-to-end via the dashboard', async ({
    apiClient,
    dashboardPage,
  }) => {
    // Seed: create an account + usage key via the API so we don't need the
    // dashboard's email widget for the smoke test.
    const stamp = Date.now();
    const account = await apiClient.newAccount({
      account_name: `e2e-smoke-${stamp}`,
      account_description: 'e2e smoke test',
    });
    const { usage_api_key: usageApiKey } = await apiClient.addUsageApiKey(account.api_key, {
      name: `e2e-smoke-${stamp}-usage`,
      execute_in_groups: [0],
    });

    await dashboardPage.goto();
    await dashboardPage.loginWithApiKey(account.api_key);
    const result = await dashboardPage.runLitAction({
      usageApiKey,
      code: HELLO_WORLD_ACTION,
    });
    expect(result.has_error).toBe(false);
    expect(result.response).toBe('Hello World!');
  });
});
