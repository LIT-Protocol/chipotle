/**
 * New-account flows — parity with `k6/correctness/new-account.spec.ts`.
 *
 * Covers:
 *   - Creating a managed account directly through the dashboard's New User /
 *     API mode card and capturing the returned API key from the banner.
 *   - Logging back in with that API key and reaching the dashboard.
 *   - Minting a usage API key via the API client (the dashboard UI gates this
 *     behind a per-row modal; we verify the route works rather than re-write
 *     every billing modal).
 */

import { test, expect } from '../../fixtures/test';

test.describe('new account (API mode)', () => {
  test('create an account through the dashboard and log back in', async ({
    dashboardPage,
    apiClient,
  }) => {
    const stamp = Date.now();
    const email = `e2e-${stamp}@example.com`;
    const name = `e2e-account-${stamp}`;

    await dashboardPage.goto();
    const apiKey = await dashboardPage.createApiModeAccount({
      email,
      name,
      description: 'created from e2e/new-account.spec.ts',
    });
    expect(apiKey).toMatch(/^[A-Za-z0-9_\-=]{16,}$/);

    // Sign out + sign back in to prove the key works.
    await dashboardPage.page.locator('#account-dropdown-trigger').click();
    await dashboardPage.page.locator('#account-signout-btn').click();
    await dashboardPage.expectLoggedOut();
    await dashboardPage.loginWithApiKey(apiKey);

    // Independent verification: the same API key should allow creating a usage
    // key via the server (same surface as k6 hits).
    const { usage_api_key } = await apiClient.addUsageApiKey(apiKey, {
      name: `e2e-usage-${stamp}`,
      execute_in_groups: [0],
    });
    expect(usage_api_key).toBeTruthy();
  });

  test('account creation is rejected when email is missing', async ({ dashboardPage }) => {
    await dashboardPage.goto();
    await dashboardPage.showNewUserTab();
    await dashboardPage.page.locator('#new-account-name').fill('missing-email');
    await dashboardPage.page.locator('#btn-create-account').click();
    await expect(dashboardPage.page.locator('#login-status')).toContainText(/email/i);
    await dashboardPage.expectLoggedOut();
  });
});
